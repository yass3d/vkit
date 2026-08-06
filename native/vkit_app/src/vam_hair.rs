use std::{
    any::Any,
    collections::{BTreeSet, VecDeque},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

use vkit_core::{
    formats::DazGeometry,
    vam::{
        AssetLocator, BuiltinHairScalp, HairGuideGeometry, HairLookPatch, HairPartReference,
        HairPreset, VaMError, load_hair_part_geometry, load_hair_part_look, load_hair_part_scalp,
        load_hair_part_settings, load_hair_scalp_textures, parse_hair_scalp_vab,
    },
};

use crate::{
    hair_preview::{
        HairPreview, HairPreviewAsset, HairScalpTextures, build_hair_preview,
        has_authored_scalp_material,
    },
    skin_preview::SkinImage,
};

const MAX_CACHED_HAIR_PARTS: usize = 16;

const MAX_SCALP_TEXTURE_EDGE: u32 = 2048;
const MAX_SHARED_SCALP_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct HairPreviewRequest {
    pub request_id: u64,
    pub preset: HairPreset,
    pub template: Arc<DazGeometry>,

    pub shared_scalp: Option<Box<HairPartReference>>,

    pub builtin_scalps: Arc<Vec<BuiltinHairScalp>>,
}

#[derive(Debug)]
pub struct HairPreviewEvent {
    pub request_id: u64,
    pub preset_id: String,
    pub outcome: Result<Arc<HairPreview>, String>,
}

#[derive(Default, Debug)]
struct HairGeometryCache {
    recent: VecDeque<(AssetLocator, Arc<HairGuideGeometry>)>,
}

impl HairGeometryCache {
    fn get(&mut self, locator: &AssetLocator) -> Option<Arc<HairGuideGeometry>> {
        let position = self.recent.iter().position(|(key, _)| key == locator)?;
        let entry = self.recent.remove(position)?;
        let geometry = Arc::clone(&entry.1);
        self.recent.push_back(entry);
        Some(geometry)
    }

    fn insert(&mut self, locator: AssetLocator, geometry: Arc<HairGuideGeometry>) {
        self.recent.retain(|(key, _)| key != &locator);
        self.recent.push_back((locator, geometry));
        while self.recent.len() > MAX_CACHED_HAIR_PARTS {
            self.recent.pop_front();
        }
    }
}

#[derive(Debug)]
pub struct HairPreviewCoordinator {
    sender: Sender<HairPreviewEvent>,
    receiver: Receiver<HairPreviewEvent>,
    active_request: Option<u64>,
    cache: Arc<Mutex<HairGeometryCache>>,
}

impl Default for HairPreviewCoordinator {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            active_request: None,
            cache: Arc::new(Mutex::new(HairGeometryCache::default())),
        }
    }
}

impl HairPreviewCoordinator {
    pub fn start(
        &mut self,
        request: HairPreviewRequest,
        wake: impl Fn() + Send + 'static,
    ) -> Result<(), String> {
        if self.active_request.is_some() {
            return Err("a VaM hair preview worker is already active".to_owned());
        }
        let request_id = request.request_id;
        let preset_id = request.preset.stable_id.clone();
        let sender = self.sender.clone();
        let cache = Arc::clone(&self.cache);
        thread::Builder::new()
            .name(format!("vkit-vam-hair-{request_id}"))
            .spawn(move || {
                let outcome = catch_unwind(AssertUnwindSafe(|| build_preview(&request, &cache)))
                    .unwrap_or_else(|payload| Err(panic_detail(payload)))
                    .map(Arc::new);
                let _ = sender.send(HairPreviewEvent {
                    request_id,
                    preset_id,
                    outcome,
                });
                wake();
            })
            .map_err(|error| format!("failed to start VaM hair preview worker: {error}"))?;
        self.active_request = Some(request_id);
        Ok(())
    }

    pub fn drain(&mut self) -> Vec<HairPreviewEvent> {
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

fn build_preview(
    request: &HairPreviewRequest,
    cache: &Mutex<HairGeometryCache>,
) -> Result<HairPreview, String> {
    let mut assets = Vec::with_capacity(request.preset.parts.len());
    let mut scalps = Vec::new();
    let mut skipped = Vec::new();
    let mut provider_materials = Vec::<(String, HairPartReference, HairLookPatch)>::new();
    for part in &request.preset.parts {
        let cached = cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&part.geometry);
        let geometry = if let Some(geometry) = cached {
            geometry
        } else {
            match load_hair_part_geometry(part) {
                Ok(geometry) => {
                    let geometry = Arc::new(geometry);
                    cache
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .insert(part.geometry.clone(), Arc::clone(&geometry));
                    geometry
                }
                Err(VaMError::UnsupportedHairGeometry { .. }) => {
                    match load_hair_part_scalp(part) {
                        Ok(scalp) => {
                            let look = load_hair_part_look(part).unwrap_or_default();
                            if scalp_material_is_visible(&look) {
                                scalps.push((Arc::new(scalp), decode_scalp_textures(part, &look)));
                            }
                        }
                        Err(_) => skipped.push(part.geometry.display_key()),
                    }
                    continue;
                }
                Err(error) => return Err(error.to_string()),
            }
        };
        let (look, physics) = load_hair_part_settings(part).map_err(|error| error.to_string())?;
        provider_materials.push((geometry.provider_name.clone(), part.clone(), look.clone()));
        assets.push(HairPreviewAsset {
            geometry,
            look,
            physics,
        });
    }

    if scalps.is_empty() {
        let mut resolved_providers = BTreeSet::new();
        for (provider_name, part, look) in &provider_materials {
            if !scalp_material_is_visible(look) {
                continue;
            }
            let provider_key = normalize_provider_name(provider_name);
            if provider_key.is_empty() || !resolved_providers.insert(provider_key.clone()) {
                continue;
            }
            let Some(provider) = request.builtin_scalps.iter().find(|candidate| {
                normalize_provider_name(&candidate.provider_name) == provider_key
            }) else {
                continue;
            };
            scalps.push((
                Arc::new(provider.geometry.clone()),
                decode_scalp_textures(part, look),
            ));
        }
    }

    if scalps.is_empty()
        && let Some(donor) = request.shared_scalp.as_ref()
        && let Ok(bytes) = donor.geometry.read_bytes(MAX_SHARED_SCALP_BYTES)
        && let Ok(scalp) = parse_hair_scalp_vab(bytes.as_slice(), &donor.geometry.display_key())
        && scalp_matches_selected_provider(&scalp, &assets)
    {
        let look = load_hair_part_look(donor).unwrap_or_default();
        if scalp_material_is_visible(&look) {
            scalps.push((Arc::new(scalp), decode_scalp_textures(donor, &look)));
        }
    }
    if assets.is_empty() && scalps.is_empty() {
        return Err(format!(
            "no part of this hairstyle could be read as strands or a scalp ({})",
            skipped.len()
        ));
    }
    let mut preview = build_hair_preview(&request.preset, &request.template, &assets, &scalps)?;
    preview.skipped_parts = skipped;
    Ok(preview)
}

fn decode_scalp_textures(part: &HairPartReference, look: &HairLookPatch) -> HairScalpTextures {
    let textures = load_hair_scalp_textures(part, look);
    let decode = |bytes: Option<Vec<u8>>| {
        bytes.and_then(|bytes| {
            SkinImage::decode_bounded(0, &bytes, MAX_SCALP_TEXTURE_EDGE)
                .ok()
                .map(Arc::new)
        })
    };
    HairScalpTextures {
        diffuse: decode(textures.diffuse),
        specular: decode(textures.specular),
        gloss: decode(textures.gloss),
        normal: decode(textures.normal),
        alpha: decode(textures.alpha),
        material: look.scalp_material_settings(),
        authored_material: has_authored_scalp_material(look),
    }
}

fn scalp_material_is_visible(look: &HairLookPatch) -> bool {
    has_authored_scalp_material(look) && look.scalp_alpha_adjust.unwrap_or(0.0) > -0.999
}

fn normalize_provider_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn scalp_matches_selected_provider(
    scalp: &vkit_core::vam::HairScalpGeometry,
    assets: &[HairPreviewAsset],
) -> bool {
    let providers = assets
        .iter()
        .map(|asset| normalize_provider_name(&asset.geometry.provider_name))
        .filter(|provider| !provider.is_empty())
        .collect::<Vec<_>>();
    scalp.materials.iter().any(|material| {
        let material = normalize_provider_name(material);
        !material.is_empty()
            && providers.iter().any(|provider| {
                material == *provider
                    || material.contains(provider.as_str())
                    || provider.contains(material.as_str())
            })
    })
}

fn panic_detail(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "hair preview worker panicked".to_owned()
    }
}
