use std::{
    any::Any,
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

use vkit_core::vam::{
    AssetLocator, G2UvMapping, SkinAuxMaterial, SkinPreset, SkinRegion, SkinTextureChannel,
    UvMaterialRegion,
};

use crate::skin_preview::{
    SkinChannel, SkinCorner, SkinImage, SkinPreview, SkinPreviewGeometry, SkinSurfaceMap,
    SkinTriangle,
};

const MAX_SKIN_IMAGE_BYTES: usize = 128 * 1024 * 1024;
const PRIMARY_SKIN_EDGE: u32 = 2048;
const AUXILIARY_SKIN_EDGE: u32 = 1024;
const SURFACE_SKIN_EDGE: u32 = 2048;
const SURFACE_AUXILIARY_EDGE: u32 = 512;

#[derive(Clone, Debug)]
pub struct SkinPreviewRequest {
    pub request_id: u64,
    pub mapping: Arc<G2UvMapping>,
    pub preset: SkinPreset,
}

#[derive(Debug)]
pub struct SkinPreviewEvent {
    pub request_id: u64,
    pub preset_id: String,
    pub outcome: Result<Arc<SkinPreview>, String>,
}

#[derive(Debug)]
pub struct SkinPreviewCoordinator {
    sender: Sender<SkinPreviewEvent>,
    receiver: Receiver<SkinPreviewEvent>,
    active_request: Option<u64>,
}

impl Default for SkinPreviewCoordinator {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            active_request: None,
        }
    }
}

impl SkinPreviewCoordinator {
    pub fn start(
        &mut self,
        request: SkinPreviewRequest,
        wake: impl Fn() + Send + 'static,
    ) -> Result<(), String> {
        if self.active_request.is_some() {
            return Err("a VaM skin preview worker is already active".to_owned());
        }
        let request_id = request.request_id;
        let preset_id = request.preset.stable_id.clone();
        let sender = self.sender.clone();
        thread::Builder::new()
            .name(format!("vkit-vam-skin-{request_id}"))
            .spawn(move || {
                let outcome = catch_unwind(AssertUnwindSafe(|| build_preview(&request)))
                    .unwrap_or_else(|payload| Err(panic_detail(payload)))
                    .map(Arc::new);
                let _ = sender.send(SkinPreviewEvent {
                    request_id,
                    preset_id,
                    outcome,
                });
                wake();
            })
            .map_err(|error| format!("failed to start VaM skin preview worker: {error}"))?;
        self.active_request = Some(request_id);
        Ok(())
    }

    pub fn drain(&mut self) -> Vec<SkinPreviewEvent> {
        let events: Vec<_> = self.receiver.try_iter().collect();
        if events
            .iter()
            .any(|event| self.active_request == Some(event.request_id))
        {
            self.active_request = None;
        }
        events
    }

    pub const fn is_active(&self) -> bool {
        self.active_request.is_some()
    }
}

type DecodedImages = BTreeMap<(AssetLocator, u32), Arc<SkinImage>>;

fn collect_decode_jobs(preset: &SkinPreset) -> Vec<(AssetLocator, u32)> {
    let mut jobs: Vec<(AssetLocator, u32)> = Vec::new();
    let mut push = |locator: Option<&AssetLocator>, edge: u32| {
        if let Some(locator) = locator {
            let key = (locator.clone(), edge);
            if !jobs.contains(&key) {
                jobs.push(key);
            }
        }
    };

    for (region, channel, locator) in preset.textures.iter() {
        if !matches!(region, SkinRegion::Face | SkinRegion::Torso) {
            continue;
        }

        let edge = if channel == SkinTextureChannel::Diffuse {
            PRIMARY_SKIN_EDGE
        } else {
            SURFACE_SKIN_EDGE
        };
        push(Some(locator), edge);
    }
    let auxiliary = &preset.auxiliary;
    for material in [
        &auxiliary.sclera,
        &auxiliary.iris,
        &auxiliary.lacrimal,
        &auxiliary.inner_mouth,
        &auxiliary.teeth,
        &auxiliary.gums,
        &auxiliary.tongue,
        &auxiliary.eyelashes,
    ] {
        push(material.diffuse.as_ref(), AUXILIARY_SKIN_EDGE);
        for locator in [
            &material.surface.normal,
            &material.surface.specular,
            &material.surface.gloss,
        ] {
            push(locator.as_ref(), SURFACE_AUXILIARY_EDGE);
        }
    }
    jobs
}

fn decode_jobs_parallel(
    revision_base: u64,
    jobs: &[(AssetLocator, u32)],
) -> (DecodedImages, f64, String) {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let results = Mutex::new(DecodedImages::new());
    let slowest = Mutex::new((0.0_f64, String::new()));
    let cursor = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(4)
        .clamp(1, 8)
        .min(jobs.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let index = cursor.fetch_add(1, Ordering::Relaxed);
                    let Some((locator, edge)) = jobs.get(index) else {
                        break;
                    };
                    let started = std::time::Instant::now();
                    let outcome =
                        decode_locator(revision_base.wrapping_add(index as u64), locator, *edge);
                    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
                    if let Ok(image) = outcome {
                        results
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .insert((locator.clone(), *edge), Arc::new(image));
                    }
                    let mut slowest = slowest
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if elapsed_ms > slowest.0 {
                        *slowest = (elapsed_ms, locator.display_key());
                    }
                }
            });
        }
    });
    let (slowest_ms, slowest_source) = slowest
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    (
        results
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
        slowest_ms,
        slowest_source,
    )
}

fn decoded_image(
    decoded: &DecodedImages,
    locator: &AssetLocator,
    edge: u32,
) -> Option<Arc<SkinImage>> {
    decoded.get(&(locator.clone(), edge)).map(Arc::clone)
}

fn build_preview(request: &SkinPreviewRequest) -> Result<SkinPreview, String> {
    let build_started = std::time::Instant::now();
    let face_locator = request
        .preset
        .diffuse(SkinRegion::Face)
        .or_else(|| request.preset.diffuse(SkinRegion::Torso))
        .ok_or_else(|| "selected VaM skin has no face or torso diffuse texture".to_owned())?;
    let torso_locator = request
        .preset
        .diffuse(SkinRegion::Torso)
        .unwrap_or(face_locator);

    let revision_base = request.request_id.wrapping_mul(128);
    let jobs = collect_decode_jobs(&request.preset);
    let (decoded, slowest_source_ms, slowest_source) = decode_jobs_parallel(revision_base, &jobs);
    let decode_ms = build_started.elapsed().as_secs_f64() * 1000.0;

    let assemble_started = std::time::Instant::now();
    let derived_base = revision_base.wrapping_add(64);
    let face = decoded_image(&decoded, face_locator, PRIMARY_SKIN_EDGE)
        .ok_or_else(|| format!("face diffuse decode failed: {}", face_locator.display_key()))?;
    let torso = if torso_locator == face_locator {
        Arc::clone(&face)
    } else {
        decoded_image(&decoded, torso_locator, PRIMARY_SKIN_EDGE).ok_or_else(|| {
            format!(
                "torso diffuse decode failed: {}",
                torso_locator.display_key()
            )
        })?
    };
    let auxiliary = &request.preset.auxiliary;
    let white = Arc::new(SkinImage::solid(derived_base.wrapping_add(15), [255; 4]));
    let auxiliary_image = |material: &SkinAuxMaterial| -> (Arc<SkinImage>, bool) {
        match material
            .diffuse
            .as_ref()
            .and_then(|locator| decoded_image(&decoded, locator, AUXILIARY_SKIN_EDGE))
        {
            Some(image) => (image, true),
            None => (Arc::clone(&white), false),
        }
    };
    let (sclera, sclera_textured) = auxiliary_image(&auxiliary.sclera);
    let (iris, iris_textured) = auxiliary_image(&auxiliary.iris);
    let (lacrimal, lacrimal_textured) = auxiliary_image(&auxiliary.lacrimal);
    let (inner_mouth, inner_mouth_textured) = auxiliary_image(&auxiliary.inner_mouth);
    let (teeth, teeth_textured) = auxiliary_image(&auxiliary.teeth);
    let (gums, gums_textured) = auxiliary_image(&auxiliary.gums);
    let (tongue, tongue_textured) = auxiliary_image(&auxiliary.tongue);
    let eyelashes = eyelash_mask_from_image(
        derived_base.wrapping_add(16),
        auxiliary
            .eyelashes
            .diffuse
            .as_ref()
            .and_then(|locator| decoded_image(&decoded, locator, AUXILIARY_SKIN_EDGE)),
    );
    let eyelashes_textured = auxiliary.eyelashes.diffuse.is_some();
    let geometry_started = std::time::Instant::now();
    let geometry = Arc::new(preview_geometry(request.request_id, &request.mapping)?);
    let geometry_ms = geometry_started.elapsed().as_secs_f64() * 1000.0;
    let surface = |revision: u64,
                   locators: &vkit_core::vam::SkinSurfaceLocators,
                   edge: u32,
                   default_specular: u8,
                   default_gloss: u8| {
        decode_surface_map_with_defaults_from(
            &decoded,
            revision,
            SurfaceMapRecipe {
                normal: locators.normal.as_ref(),
                specular: locators.specular.as_ref(),
                gloss: locators.gloss.as_ref(),
                max_edge: edge,
                default_specular,
                default_gloss,
            },
        )
    };
    let face_surface = surface(
        derived_base.wrapping_add(20),
        &request.preset.surface(SkinRegion::Face),
        SURFACE_SKIN_EDGE,
        96,
        140,
    );
    let torso_surface = surface(
        derived_base.wrapping_add(21),
        &request.preset.surface(SkinRegion::Torso),
        SURFACE_SKIN_EDGE,
        96,
        140,
    );
    let inner_mouth_surface = surface(
        derived_base.wrapping_add(26),
        &auxiliary.inner_mouth.surface,
        SURFACE_AUXILIARY_EDGE,
        88,
        166,
    );
    let teeth_surface = surface(
        derived_base.wrapping_add(27),
        &auxiliary.teeth.surface,
        SURFACE_AUXILIARY_EDGE,
        106,
        148,
    );
    let gums_surface = surface(
        derived_base.wrapping_add(28),
        &auxiliary.gums.surface,
        SURFACE_AUXILIARY_EDGE,
        86,
        158,
    );
    let tongue_surface = surface(
        derived_base.wrapping_add(29),
        &auxiliary.tongue.surface,
        SURFACE_AUXILIARY_EDGE,
        92,
        174,
    );
    let mouth_surface_atlas = pack_mouth_surface_atlas(
        derived_base.wrapping_add(22),
        [
            &inner_mouth_surface,
            &teeth_surface,
            &gums_surface,
            &tongue_surface,
        ],
    )?;
    let preview = SkinPreview {
        revision: request.request_id,
        geometry,
        face,
        torso,
        sclera,
        iris,
        lacrimal,
        inner_mouth,
        teeth,
        gums,
        tongue,
        eyelashes,
        face_surface,
        torso_surface,
        mouth_surface_atlas,
        sclera_surface: surface(
            derived_base.wrapping_add(23),
            &auxiliary.sclera.surface,
            SURFACE_AUXILIARY_EDGE,
            173,
            194,
        ),
        iris_surface: surface(
            derived_base.wrapping_add(24),
            &auxiliary.iris.surface,
            SURFACE_AUXILIARY_EDGE,
            148,
            209,
        ),
        lacrimal_surface: surface(
            derived_base.wrapping_add(25),
            &auxiliary.lacrimal.surface,
            SURFACE_AUXILIARY_EDGE,
            89,
            148,
        ),
        auxiliary_colors: [
            auxiliary.sclera.base_color,
            auxiliary.iris.base_color,
            auxiliary.lacrimal.base_color,
            auxiliary.inner_mouth.base_color,
            auxiliary.teeth.base_color,
            auxiliary.gums.base_color,
            auxiliary.tongue.base_color,
            auxiliary.eyelashes.base_color,
        ],
        auxiliary_textured: [
            sclera_textured,
            iris_textured,
            lacrimal_textured,
            inner_mouth_textured,
            teeth_textured,
            gums_textured,
            tongue_textured,
            eyelashes_textured,
        ],
    };
    let assemble_ms = (assemble_started.elapsed().as_secs_f64() * 1000.0 - geometry_ms).max(0.0);
    let total_ms = build_started.elapsed().as_secs_f64() * 1000.0;
    if let Ok(log) = crate::diagnostics::global_log() {
        let _ = log.record(
            crate::diagnostics::Severity::Info,
            "skin",
            "skin_preview_built",
            &format!(
                "preset={}; total_ms={total_ms:.0}; decode_ms={decode_ms:.0}; assemble_ms={assemble_ms:.0}; geometry_ms={geometry_ms:.0}; sources={}; decoded={}; slowest_source_ms={slowest_source_ms:.0}; slowest_source={slowest_source}",
                request.preset.stable_id,
                jobs.len(),
                decoded.len(),
            ),
        );
    }
    Ok(preview)
}

pub fn decode_skin_texture(
    revision: u64,
    locator: &AssetLocator,
    max_edge: u32,
) -> Result<SkinImage, String> {
    decode_locator(revision, locator, max_edge)
}

fn decode_locator(
    revision: u64,
    locator: &AssetLocator,
    max_edge: u32,
) -> Result<SkinImage, String> {
    if let AssetLocator::File(path) = locator
        && path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("vamcachemeta"))
    {
        return SkinImage::decode_vam_cache_bounded(revision, path, max_edge);
    }
    if let AssetLocator::BuiltinTexture(reference) = locator {
        let decoded = vkit_core::vam::load_builtin_texture_rgba(reference, max_edge)
            .map_err(|error| error.to_string())?;
        return SkinImage::new(revision, decoded.width, decoded.height, decoded.rgba8);
    }
    let bytes = locator
        .read_bytes(MAX_SKIN_IMAGE_BYTES)
        .map_err(|error| error.to_string())?;
    SkinImage::decode_bounded(revision, &bytes, max_edge)
}

#[cfg(test)]
fn decode_auxiliary_cached(
    material: &SkinAuxMaterial,
    cache: &mut BTreeMap<AssetLocator, Arc<SkinImage>>,
    next_revision: &mut u64,
    white: &Arc<SkinImage>,
    max_edge: u32,
) -> (Arc<SkinImage>, bool) {
    let Some(locator) = material.diffuse.as_ref() else {
        return (Arc::clone(white), false);
    };
    if let Some(image) = cache.get(locator) {
        return (Arc::clone(image), true);
    }
    let Ok(image) = decode_locator(*next_revision, locator, max_edge) else {
        return (Arc::clone(white), false);
    };
    *next_revision = (*next_revision).wrapping_add(1);
    let image = Arc::new(image);
    cache.insert(locator.clone(), Arc::clone(&image));
    (image, true)
}

#[cfg(test)]
fn decode_eyelash_mask(revision: u64, material: &SkinAuxMaterial, max_edge: u32) -> Arc<SkinImage> {
    eyelash_mask_from_image(
        revision,
        material.diffuse.as_ref().and_then(|locator| {
            decode_locator(revision, locator, max_edge)
                .ok()
                .map(Arc::new)
        }),
    )
}

fn eyelash_mask_from_image(revision: u64, source: Option<Arc<SkinImage>>) -> Arc<SkinImage> {
    let Some(source) = source else {
        return Arc::new(SkinImage::solid(revision, [255, 255, 255, 0]));
    };
    let has_meaningful_alpha = source.rgba8.chunks_exact(4).any(|pixel| pixel[3] < 250);
    let mut rgba = Vec::with_capacity(source.rgba8.len());
    for pixel in source.rgba8.chunks_exact(4) {
        let mask = if has_meaningful_alpha {
            pixel[3]
        } else {
            ((u16::from(pixel[0]) * 54 + u16::from(pixel[1]) * 183 + u16::from(pixel[2]) * 19)
                / 256) as u8
        };
        rgba.extend_from_slice(&[255, 255, 255]);
        rgba.push(mask);
    }
    let mut mask = (*source).clone();
    mask.revision = revision;
    mask.rgba8 = Arc::new(rgba);
    Arc::new(mask)
}

#[cfg(test)]
fn decode_surface_map(
    revision: u64,
    normal: Option<&AssetLocator>,
    specular: Option<&AssetLocator>,
    gloss: Option<&AssetLocator>,
    max_edge: u32,
) -> SkinSurfaceMap {
    decode_surface_map_with_defaults(
        revision,
        SurfaceMapRecipe {
            normal,
            specular,
            gloss,
            max_edge,
            default_specular: 96,
            default_gloss: 140,
        },
    )
}

struct SurfaceMapRecipe<'a> {
    normal: Option<&'a AssetLocator>,
    specular: Option<&'a AssetLocator>,
    gloss: Option<&'a AssetLocator>,

    max_edge: u32,
    default_specular: u8,
    default_gloss: u8,
}

fn normal_strength(normal: Option<&AssetLocator>) -> f32 {
    match normal {
        Some(AssetLocator::BuiltinTexture(_)) => BUILTIN_NORMAL_STRENGTH,
        _ => 1.0,
    }
}

const BUILTIN_NORMAL_STRENGTH: f32 = 0.5;

#[cfg(test)]
fn decode_surface_map_with_defaults(revision: u64, recipe: SurfaceMapRecipe<'_>) -> SkinSurfaceMap {
    decode_surface_map_with_defaults_from(&DecodedImages::new(), revision, recipe)
}

fn decode_surface_map_with_defaults_from(
    decoded: &DecodedImages,
    revision: u64,
    recipe: SurfaceMapRecipe<'_>,
) -> SkinSurfaceMap {
    let SurfaceMapRecipe {
        normal,
        specular,
        gloss,
        max_edge,
        default_specular,
        default_gloss,
    } = recipe;

    let recipe = SurfaceMapRecipe {
        normal,
        specular,
        gloss,
        max_edge,
        default_specular,
        default_gloss,
    };
    let allow_direct_decode = decoded.is_empty();
    let decode = |locator: Option<&AssetLocator>| {
        locator.and_then(|locator| {
            decoded_image(decoded, locator, max_edge).or_else(|| {
                allow_direct_decode
                    .then(|| {
                        decode_locator(revision, locator, max_edge)
                            .ok()
                            .map(Arc::new)
                    })
                    .flatten()
            })
        })
    };
    let normal = decode(normal);
    let specular = decode(specular);
    let gloss = decode(gloss);
    let width = [&normal, &specular, &gloss]
        .into_iter()
        .filter_map(|image| image.as_ref().map(|image| image.width))
        .max()
        .unwrap_or(1);
    let height = [&normal, &specular, &gloss]
        .into_iter()
        .filter_map(|image| image.as_ref().map(|image| image.height))
        .max()
        .unwrap_or(1);
    fn view(image: &Option<Arc<SkinImage>>) -> Option<vkit_core::pixels::RgbaView<'_>> {
        image.as_ref().and_then(|image| {
            vkit_core::pixels::RgbaView::new(&image.rgba8, image.width, image.height).ok()
        })
    }
    let packed = vkit_core::pixels::pack_surface_map(
        view(&normal),
        view(&specular),
        view(&gloss),
        width,
        height,
        vkit_core::pixels::SurfacePackSettings {
            default_specular,
            default_gloss,
            normal_strength: normal_strength(recipe.normal),
        },
    );
    let image = SkinImage::new(revision, width, height, packed).unwrap_or_else(|_| {
        SkinImage::solid(revision, [128, 128, default_specular, default_gloss])
    });
    SkinSurfaceMap {
        packed: Arc::new(image),
    }
}

fn pack_mouth_surface_atlas(
    revision: u64,
    surfaces: [&SkinSurfaceMap; 4],
) -> Result<SkinSurfaceMap, String> {
    let tile_width = surfaces
        .iter()
        .map(|surface| surface.packed.width)
        .max()
        .unwrap_or(1);
    let tile_height = surfaces
        .iter()
        .map(|surface| surface.packed.height)
        .max()
        .unwrap_or(1);
    let width = tile_width
        .checked_mul(2)
        .ok_or_else(|| "mouth surface atlas width overflow".to_owned())?;
    let height = tile_height
        .checked_mul(2)
        .ok_or_else(|| "mouth surface atlas height overflow".to_owned())?;
    let byte_count = usize::try_from(u64::from(width) * u64::from(height) * 4)
        .map_err(|_| "mouth surface atlas byte count exceeds usize".to_owned())?;
    let mut packed = vec![0_u8; byte_count];
    for (tile, surface) in surfaces.into_iter().enumerate() {
        let tile_x = (tile as u32 % 2) * tile_width;
        let tile_y = (tile as u32 / 2) * tile_height;
        let source = vkit_core::pixels::RgbaView::new(
            &surface.packed.rgba8,
            surface.packed.width,
            surface.packed.height,
        )
        .map_err(|error| format!("mouth surface tile view failed: {error}"))?;
        vkit_core::pixels::blit_resized_tile(
            &mut packed,
            width,
            source,
            vkit_core::pixels::AtlasTile {
                x: tile_x,
                y: tile_y,
                width: tile_width,
                height: tile_height,
            },
        );
    }
    Ok(SkinSurfaceMap {
        packed: Arc::new(SkinImage::new(revision, width, height, packed)?),
    })
}

pub(crate) const fn is_eye_attachment(region: UvMaterialRegion) -> bool {
    matches!(
        region,
        UvMaterialRegion::Lacrimal | UvMaterialRegion::Tear | UvMaterialRegion::Eyelashes
    )
}

fn preview_geometry(revision: u64, mapping: &G2UvMapping) -> Result<SkinPreviewGeometry, String> {
    let triangles = mapping
        .triangles
        .iter()
        .filter_map(|triangle| {
            if !triangle.on_head && !is_eye_attachment(triangle.material_region) {
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
        .collect();
    SkinPreviewGeometry::new(revision, triangles)
}

fn panic_detail(payload: Box<dyn Any + Send>) -> String {
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("unknown panic payload");
    format!("VaM skin preview worker stopped unexpectedly: {message}")
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, fs, io::Cursor, path::PathBuf};

    use vkit_core::{
        formats::load_dsf_path,
        vam::{
            G2UvTriangle, UvMaterialRegion, VaMRoot, load_g2_uv_mapping,
            scan_skin_library_with_report,
        },
    };

    use super::*;

    fn configured_vam_root() -> Option<PathBuf> {
        std::env::var_os("VKIT_VAM_ROOT").map(PathBuf::from)
    }

    fn configured_g2_dsf() -> Option<PathBuf> {
        std::env::var_os("VKIT_G2_DSF").map(PathBuf::from)
    }

    fn configured_vam_sources() -> Option<(PathBuf, PathBuf)> {
        match (configured_vam_root(), configured_g2_dsf()) {
            (Some(root), Some(dsf)) => Some((root, dsf)),
            _ => {
                eprintln!("set VKIT_VAM_ROOT and VKIT_G2_DSF to run this");
                None
            }
        }
    }

    #[test]
    fn the_skin_overlay_drops_every_triangle_that_is_not_on_the_head() {
        let triangle = |region, on_head| G2UvTriangle {
            canonical_face_index: 0,
            canonical_triangle_index: 0,
            material_region: region,
            on_head,
            position_indices: [0, 1, 2],
            uvs: [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
        };
        let mapping = G2UvMapping {
            source_path: PathBuf::from("femalecustom.obj"),
            coordinate_rms_cm: 0.0,
            coordinate_max_cm: 0.0,
            uncovered_triangles: 0,
            faces: Vec::new(),
            triangles: vec![
                triangle(UvMaterialRegion::Torso, true),
                triangle(UvMaterialRegion::Torso, false),
                triangle(UvMaterialRegion::Face, true),
                triangle(UvMaterialRegion::Limbs, false),
            ],
        };
        let converted = preview_geometry(1, &mapping).unwrap();
        assert_eq!(
            converted.triangles.len(),
            2,
            "only the two on-head triangles survive"
        );
        assert!(
            converted
                .triangles
                .iter()
                .all(|triangle| matches!(triangle.channel, SkinChannel::Face | SkinChannel::Torso))
        );
    }

    #[test]
    fn the_shells_attached_to_the_lid_reach_the_renderer() {
        let triangle = |region| G2UvTriangle {
            canonical_face_index: 0,
            canonical_triangle_index: 0,
            material_region: region,

            on_head: false,
            position_indices: [0, 1, 2],
            uvs: [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
        };
        let mapping = G2UvMapping {
            source_path: PathBuf::from("femalecustom.obj"),
            coordinate_rms_cm: 0.0,
            coordinate_max_cm: 0.0,
            uncovered_triangles: 0,
            faces: Vec::new(),
            triangles: vec![
                triangle(UvMaterialRegion::Lacrimal),
                triangle(UvMaterialRegion::Tear),
                triangle(UvMaterialRegion::Eyelashes),
                triangle(UvMaterialRegion::Limbs),
                triangle(UvMaterialRegion::Torso),
            ],
        };
        let channels: Vec<_> = preview_geometry(1, &mapping)
            .unwrap()
            .triangles
            .iter()
            .map(|triangle| triangle.channel)
            .collect();
        assert_eq!(
            channels,
            vec![
                SkinChannel::Lacrimal,
                SkinChannel::Tear,
                SkinChannel::Eyelashes
            ]
        );
    }

    #[test]
    fn uv_mapping_converts_without_changing_position_ids() {
        let mapping = G2UvMapping {
            source_path: PathBuf::from("femalecustom.obj"),
            coordinate_rms_cm: 0.0,
            coordinate_max_cm: 0.0,
            uncovered_triangles: 0,
            faces: Vec::new(),
            triangles: vec![G2UvTriangle {
                canonical_face_index: 4,
                canonical_triangle_index: 7,
                material_region: UvMaterialRegion::Face,
                on_head: true,
                position_indices: [9, 2, 7],
                uvs: [[0.0, 0.0], [0.5, 0.25], [1.0, 1.0]],
            }],
        };
        let converted = preview_geometry(3, &mapping).unwrap();
        assert_eq!(converted.triangles[0].channel, SkinChannel::Face);
        assert_eq!(
            converted.triangles[0]
                .corners
                .map(|corner| corner.vertex_id),
            [9, 2, 7]
        );
    }

    #[test]
    fn rgb_eyelash_mask_does_not_become_a_solid_card() {
        let directory =
            std::env::temp_dir().join(format!("vkit-eyelash-mask-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("lashes.png");
        let source = image::RgbaImage::from_fn(2, 1, |x, _| {
            if x == 0 {
                image::Rgba([255, 255, 255, 255])
            } else {
                image::Rgba([0, 0, 0, 255])
            }
        });
        let mut encoded = Vec::new();
        image::DynamicImage::ImageRgba8(source)
            .write_to(&mut Cursor::new(&mut encoded), image::ImageFormat::Png)
            .unwrap();
        fs::write(&path, encoded).unwrap();
        let material = SkinAuxMaterial {
            diffuse: Some(AssetLocator::File(path)),
            diffuse_source: Default::default(),
            surface: vkit_core::vam::SkinSurfaceLocators::default(),
            base_color: [24, 12, 6, 255],
        };
        let image = decode_eyelash_mask(4, &material, AUXILIARY_SKIN_EDGE);
        assert_eq!(&image.rgba8[..4], &[255, 255, 255, 255]);
        assert_eq!(&image.rgba8[4..8], &[255, 255, 255, 0]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn missing_eyelash_mask_is_fully_transparent() {
        let material = SkinAuxMaterial {
            diffuse: None,
            diffuse_source: Default::default(),
            surface: vkit_core::vam::SkinSurfaceLocators::default(),
            base_color: [24, 12, 6, 255],
        };
        let image = decode_eyelash_mask(4, &material, AUXILIARY_SKIN_EDGE);
        assert_eq!(&image.rgba8[..4], &[255, 255, 255, 0]);
    }

    #[test]
    fn identical_auxiliary_locators_share_one_decoded_image() {
        let directory = std::env::temp_dir().join(format!("vkit-aux-tint-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("teeth.png");
        let source = image::RgbaImage::from_pixel(1, 1, image::Rgba([200, 100, 50, 255]));
        let mut encoded = Vec::new();
        image::DynamicImage::ImageRgba8(source)
            .write_to(&mut Cursor::new(&mut encoded), image::ImageFormat::Png)
            .unwrap();
        fs::write(&path, encoded).unwrap();
        let locator = AssetLocator::File(path);
        let first_material = SkinAuxMaterial {
            diffuse: Some(locator.clone()),
            diffuse_source: Default::default(),
            surface: vkit_core::vam::SkinSurfaceLocators::default(),
            base_color: [128, 255, 128, 255],
        };
        let second_material = SkinAuxMaterial {
            diffuse: Some(locator),
            diffuse_source: Default::default(),
            surface: vkit_core::vam::SkinSurfaceLocators::default(),
            base_color: [255, 128, 128, 255],
        };
        let white = Arc::new(SkinImage::solid(99, [255; 4]));
        let mut cache = BTreeMap::new();
        let mut revision = 5;
        let (first, first_textured) = decode_auxiliary_cached(
            &first_material,
            &mut cache,
            &mut revision,
            &white,
            AUXILIARY_SKIN_EDGE,
        );
        let (second, second_textured) = decode_auxiliary_cached(
            &second_material,
            &mut cache,
            &mut revision,
            &white,
            AUXILIARY_SKIN_EDGE,
        );
        assert!(Arc::ptr_eq(&first, &second));
        assert!(first_textured && second_textured);
        assert_eq!(revision, 6);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn pbr_surface_pack_preserves_normal_xy_specular_and_gloss_channels() {
        let directory =
            std::env::temp_dir().join(format!("vkit-surface-pack-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let write = |name: &str, color: [u8; 4]| {
            let path = directory.join(name);
            let source = image::RgbaImage::from_pixel(2, 1, image::Rgba(color));
            let mut encoded = Vec::new();
            image::DynamicImage::ImageRgba8(source)
                .write_to(&mut Cursor::new(&mut encoded), image::ImageFormat::Png)
                .unwrap();
            fs::write(&path, encoded).unwrap();
            AssetLocator::File(path)
        };
        let normal = write("faceN.png", [230, 17, 250, 255]);
        let specular = write("faceS.png", [80, 80, 80, 255]);
        let gloss = write("faceG.png", [200, 200, 200, 255]);
        let surface = decode_surface_map(
            73,
            Some(&normal),
            Some(&specular),
            Some(&gloss),
            SURFACE_SKIN_EDGE,
        );
        assert_eq!((surface.packed.width, surface.packed.height), (2, 1));
        assert_eq!(&surface.packed.rgba8[..4], &[230, 17, 80, 200]);
        assert_eq!(surface.packed.revision, 73);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn mouth_surface_atlas_has_stable_top_left_to_bottom_right_tile_order() {
        let surface = |revision, pixel| SkinSurfaceMap {
            packed: Arc::new(SkinImage::solid(revision, pixel)),
        };
        let inner_mouth = surface(1, [10, 11, 12, 13]);
        let teeth = surface(2, [20, 21, 22, 23]);
        let gums = surface(3, [30, 31, 32, 33]);
        let tongue = surface(4, [40, 41, 42, 43]);

        let atlas = pack_mouth_surface_atlas(99, [&inner_mouth, &teeth, &gums, &tongue]).unwrap();
        assert_eq!((atlas.packed.width, atlas.packed.height), (2, 2));
        assert_eq!(atlas.packed.revision, 99);
        assert_eq!(
            atlas.packed.rgba8.as_slice(),
            &[
                10, 11, 12, 13, 20, 21, 22, 23, 30, 31, 32, 33, 40, 41, 42, 43,
            ]
        );
    }

    #[test]
    fn missing_eye_surface_maps_keep_material_specific_pbr_defaults() {
        let sclera = decode_surface_map_with_defaults(
            1,
            SurfaceMapRecipe {
                normal: None,
                specular: None,
                gloss: None,
                max_edge: 512,
                default_specular: 173,
                default_gloss: 194,
            },
        );
        let iris = decode_surface_map_with_defaults(
            2,
            SurfaceMapRecipe {
                normal: None,
                specular: None,
                gloss: None,
                max_edge: 512,
                default_specular: 148,
                default_gloss: 209,
            },
        );
        let lacrimal = decode_surface_map_with_defaults(
            3,
            SurfaceMapRecipe {
                normal: None,
                specular: None,
                gloss: None,
                max_edge: 512,
                default_specular: 89,
                default_gloss: 148,
            },
        );

        assert_eq!(sclera.packed.rgba8.as_slice(), &[128, 128, 173, 194]);
        assert_eq!(iris.packed.rgba8.as_slice(), &[128, 128, 148, 209]);
        assert_eq!(lacrimal.packed.rgba8.as_slice(), &[128, 128, 89, 148]);
    }

    #[test]
    #[ignore = "requires the user's local VaM installation and licensed G2F DSF"]
    fn decodes_a_real_external_vam_skin_without_touching_export_state() {
        let Some((root_path, dsf_path)) = configured_vam_sources() else {
            return;
        };
        let root = VaMRoot::open(root_path).unwrap();
        let geometry = load_dsf_path(dsf_path, 0).unwrap();
        let mapping = load_g2_uv_mapping(&root, &geometry).unwrap();
        assert_eq!(mapping.triangles.len(), 16_756);
        let regions: BTreeSet<_> = mapping
            .triangles
            .iter()
            .map(|triangle| triangle.material_region)
            .collect();
        assert_eq!(regions.len(), 14);
        let preset = scan_skin_library_with_report(&root)
            .unwrap()
            .presets
            .into_iter()
            .find(|preset| {
                preset.diffuse(SkinRegion::Face).is_some()
                    && preset.surface(SkinRegion::Face).normal.is_some()
                    && preset.surface(SkinRegion::Face).specular.is_some()
                    && preset.surface(SkinRegion::Face).gloss.is_some()
            })
            .expect("real VaM skin library should contain a complete face PBR texture set");
        let preview = build_preview(&SkinPreviewRequest {
            request_id: 41,
            mapping: Arc::new(mapping),
            preset,
        })
        .unwrap();
        assert!(preview.face.width > 0 && preview.face.height > 0);
        assert_eq!(preview.geometry.triangles.len(), 16_756);
        assert!(preview.iris.width > 0 && preview.inner_mouth.width > 0);
        assert!(preview.face.width <= PRIMARY_SKIN_EDGE);
        assert!(preview.face.height <= PRIMARY_SKIN_EDGE);
        assert!(preview.torso.width <= PRIMARY_SKIN_EDGE);
        assert!(preview.torso.height <= PRIMARY_SKIN_EDGE);
        assert!(preview.face_surface.packed.width > 1);
        assert!(preview.face_surface.packed.height > 1);
        assert_eq!(preview.mouth_surface_atlas.packed.width % 2, 0);
        assert_eq!(preview.mouth_surface_atlas.packed.height % 2, 0);
        assert!(preview.mouth_surface_atlas.packed.width <= SURFACE_AUXILIARY_EDGE * 2);
        assert!(preview.mouth_surface_atlas.packed.height <= SURFACE_AUXILIARY_EDGE * 2);
        let auxiliary_images = [
            &preview.sclera,
            &preview.iris,
            &preview.lacrimal,
            &preview.inner_mouth,
            &preview.teeth,
            &preview.gums,
            &preview.tongue,
            &preview.eyelashes,
        ];
        assert!(auxiliary_images.iter().all(|image| {
            image.width <= AUXILIARY_SKIN_EDGE && image.height <= AUXILIARY_SKIN_EDGE
        }));
        let all_images = [
            &preview.face,
            &preview.torso,
            &preview.sclera,
            &preview.iris,
            &preview.lacrimal,
            &preview.inner_mouth,
            &preview.teeth,
            &preview.gums,
            &preview.tongue,
            &preview.eyelashes,
            &preview.face_surface.packed,
            &preview.torso_surface.packed,
            &preview.mouth_surface_atlas.packed,
            &preview.sclera_surface.packed,
            &preview.iris_surface.packed,
            &preview.lacrimal_surface.packed,
        ];
        let mut unique = BTreeSet::new();
        let resident_bytes: usize = all_images
            .into_iter()
            .filter(|image| unique.insert(Arc::as_ptr(image) as usize))
            .map(|image| image.rgba8.len())
            .sum();
        assert!(resident_bytes <= 96 * 1024 * 1024);
        eprintln!(
            "VaM skin preview retains {:.1} MiB across {} unique CPU images",
            resident_bytes as f64 / (1024.0 * 1024.0),
            unique.len()
        );
    }

    #[test]
    #[ignore = "requires the user's local VaM installation and licensed G2F DSF"]
    fn builds_a_builtin_vam_skin_preview_from_the_real_install() {
        use std::time::Instant;
        let Some((root_path, dsf_path)) = configured_vam_sources() else {
            return;
        };
        let root = VaMRoot::open(root_path).unwrap();
        let geometry = load_dsf_path(dsf_path, 0).unwrap();
        let mapping = Arc::new(load_g2_uv_mapping(&root, &geometry).unwrap());
        let presets = scan_skin_library_with_report(&root).unwrap().presets;
        let builtin: Vec<_> = presets
            .iter()
            .filter(|preset| preset.stable_id.starts_with("vam:skin:builtin:"))
            .collect();
        assert!(builtin.len() >= 12, "built-ins listed: {}", builtin.len());
        let victoria = builtin
            .iter()
            .find(|preset| preset.label.contains("Victoria"))
            .expect("Victoria ships with VaM");
        let started = Instant::now();
        let preview = build_preview(&SkinPreviewRequest {
            request_id: 77,
            mapping: Arc::clone(&mapping),
            preset: (*victoria).clone(),
        })
        .unwrap();
        eprintln!(
            "builtin Victoria preview: {:.0}ms face={}x{} torso={}x{}",
            started.elapsed().as_secs_f64() * 1000.0,
            preview.face.width,
            preview.face.height,
            preview.torso.width,
            preview.torso.height
        );
        assert!(preview.face.width >= 1024 && preview.face.width <= PRIMARY_SKIN_EDGE);
        assert!(preview.torso.width >= 1024 && preview.torso.width <= PRIMARY_SKIN_EDGE);

        assert!(preview.face_surface.packed.width > 1);
        assert!(preview.torso_surface.packed.width > 1);

        assert!(preview.auxiliary_textured[3], "inner mouth texture");
        assert!(preview.auxiliary_textured[4], "teeth texture");
        let warm = Instant::now();
        let _ = build_preview(&SkinPreviewRequest {
            request_id: 78,
            mapping,
            preset: (*victoria).clone(),
        })
        .unwrap();
        eprintln!(
            "builtin Victoria preview warm (fmskintex cache): {:.0}ms",
            warm.elapsed().as_secs_f64() * 1000.0
        );
    }

    #[test]
    #[ignore = "diagnostic: requires the user's local VaM installation and licensed G2F DSF"]
    fn real_vam_eye_and_lash_cache_orientation_stays_direct_v() {
        fn cache_meta(root: &std::path::Path, needle: &str) -> PathBuf {
            let mut matches: Vec<_> = fs::read_dir(root.join("Cache").join("Textures"))
                .unwrap()
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension()
                        .and_then(|value| value.to_str())
                        .is_some_and(|value| value.eq_ignore_ascii_case("vamcachemeta"))
                        && path
                            .file_stem()
                            .and_then(|value| value.to_str())
                            .is_some_and(|value| value.to_ascii_lowercase().contains(needle))
                })
                .collect();
            matches.sort();
            matches.into_iter().next().expect("known VaM cache fixture")
        }

        fn region_mean(
            image: &SkinImage,
            mapping: &G2UvMapping,
            region: UvMaterialRegion,
            channel: SkinChannel,
            extra_flip: bool,
            alpha_mask: bool,
        ) -> f64 {
            let mut sum = 0.0;
            let mut count = 0_u64;
            for triangle in mapping
                .triangles
                .iter()
                .filter(|triangle| triangle.material_region == region)
            {
                let source_uv = [
                    triangle.uvs.iter().map(|uv| uv[0]).sum::<f32>() / 3.0,
                    triangle.uvs.iter().map(|uv| uv[1]).sum::<f32>() / 3.0,
                ];
                let mut uv = channel.texture_uv(source_uv, image.uv_orientation);
                if extra_flip {
                    uv[1] = 1.0 - uv[1];
                }
                let x =
                    (uv[0].clamp(0.0, 1.0) * image.width.saturating_sub(1) as f32).round() as usize;
                let y = (uv[1].clamp(0.0, 1.0) * image.height.saturating_sub(1) as f32).round()
                    as usize;
                let offset = (y * image.width as usize + x) * 4;
                let pixel = &image.rgba8[offset..offset + 4];
                let value = if alpha_mask {
                    u32::from(pixel[3])
                } else {
                    (u32::from(pixel[0]) * 54
                        + u32::from(pixel[1]) * 183
                        + u32::from(pixel[2]) * 19)
                        / 256
                };
                sum += f64::from(value) / 255.0;
                count += 1;
            }
            assert!(count > 0, "fixture region must contain triangles");
            sum / count as f64
        }

        let Some((root_path, dsf_path)) = configured_vam_sources() else {
            return;
        };
        let root_path = root_path.as_path();
        let root = VaMRoot::open(root_path).unwrap();
        let geometry = load_dsf_path(dsf_path, 0).unwrap();
        let mapping = load_g2_uv_mapping(&root, &geometry).unwrap();
        let eye = decode_locator(
            901,
            &AssetLocator::File(cache_meta(root_path, "v5breeeyes8m")),
            AUXILIARY_SKIN_EDGE,
        )
        .unwrap();
        let lash = decode_eyelash_mask(
            902,
            &SkinAuxMaterial {
                diffuse: Some(AssetLocator::File(cache_meta(root_path, "v5breelashes1"))),
                diffuse_source: Default::default(),
                surface: vkit_core::vam::SkinSurfaceLocators::default(),
                base_color: [22, 15, 11, 255],
            },
            AUXILIARY_SKIN_EDGE,
        );

        let sclera_direct = region_mean(
            &eye,
            &mapping,
            UvMaterialRegion::Sclera,
            SkinChannel::Sclera,
            false,
            false,
        );
        let sclera_flipped = region_mean(
            &eye,
            &mapping,
            UvMaterialRegion::Sclera,
            SkinChannel::Sclera,
            true,
            false,
        );
        let lash_direct = region_mean(
            &lash,
            &mapping,
            UvMaterialRegion::Eyelashes,
            SkinChannel::Eyelashes,
            false,
            true,
        );
        let lash_flipped = region_mean(
            &lash,
            &mapping,
            UvMaterialRegion::Eyelashes,
            SkinChannel::Eyelashes,
            true,
            true,
        );
        assert!(sclera_direct > 0.45 && sclera_direct > sclera_flipped * 2.0);
        assert!(lash_direct > lash_flipped * 1.25);
    }
}

#[cfg(test)]
mod limbs_evidence {}
