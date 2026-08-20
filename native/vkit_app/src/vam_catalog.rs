use std::{
    any::Any,
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use vkit_core::{
    formats::{DazGeometry, MorphAuthoring, MorphTarget},
    vam::{
        BuiltinHairScalp, G2UvMapping, HairPartReference, HairPreset, PackageIndex, SkinPreset,
        SkinSex, VaMRoot, load_builtin_hair_scalps, load_g2_uv_mapping_for_sex,
        scan_hair_presets_with_report, scan_skin_library_with_report,
    },
};

use crate::{
    vam_edit_sources::{VaMEditSource, scan_edit_sources},
    vam_morph_cache::{CachedBuiltinMorph, MorphCacheReceipt},
    vam_morph_index::VaMMorphIndex,
};

#[derive(Clone, Debug)]
pub enum VaMWorkRequest {
    ScanAppearance {
        catalog_revision: u64,
        requested_root: PathBuf,
        figure_sex: SkinSex,
        template_geometry: Option<Arc<DazGeometry>>,

        force_rescan: bool,
    },
    LoadMorphCache {
        catalog_revision: u64,
        figure_sex: SkinSex,
        template_geometry: Arc<DazGeometry>,

        vam_root: PathBuf,
    },
    ResolveCachedMorph {
        catalog_revision: u64,
        geometry_revision: u64,
        control_id: String,
        descriptor: Box<CachedBuiltinMorph>,
        vertex_count: usize,
        canonical_faces: Arc<Vec<Vec<u32>>>,
    },
}

impl VaMWorkRequest {
    const fn catalog_revision(&self) -> u64 {
        match self {
            Self::ScanAppearance {
                catalog_revision, ..
            }
            | Self::LoadMorphCache {
                catalog_revision, ..
            }
            | Self::ResolveCachedMorph {
                catalog_revision, ..
            } => *catalog_revision,
        }
    }

    pub fn failure_event(self, detail: String) -> VaMWorkerEvent {
        match self {
            Self::ScanAppearance {
                catalog_revision, ..
            } => VaMWorkerEvent::Appearance {
                catalog_revision,
                outcome: Err(detail),
            },
            Self::LoadMorphCache {
                catalog_revision, ..
            } => VaMWorkerEvent::MorphCatalog {
                catalog_revision,
                outcome: Err(detail),
            },
            Self::ResolveCachedMorph {
                catalog_revision,
                geometry_revision,
                control_id,
                ..
            } => VaMWorkerEvent::Morph {
                catalog_revision,
                geometry_revision,
                control_id,
                outcome: Err(detail),
            },
        }
    }

    pub const fn is_appearance_scan(&self) -> bool {
        matches!(self, Self::ScanAppearance { .. })
    }

    const fn thread_name(&self) -> &'static str {
        match self {
            Self::ScanAppearance { .. } => "vkit-vam-appearance",
            Self::LoadMorphCache { .. } | Self::ResolveCachedMorph { .. } => "vkit-embedded-morph",
        }
    }
}

#[derive(Debug)]
pub struct VaMMorphPayload {
    pub target: Arc<MorphTarget>,
    pub unsupported_formula_count: usize,
}

#[derive(Clone, Debug)]
pub struct VaMCatalogPayload {
    pub canonical_root: PathBuf,
    pub skin_presets: Vec<SkinPreset>,

    pub hair_presets: Vec<HairPreset>,

    pub shared_scalp: Option<Box<HairPartReference>>,

    pub builtin_hair_scalps: Arc<Vec<BuiltinHairScalp>>,
    pub builtin_scalp_textures: Arc<Vec<vkit_core::vam::BuiltinScalpTextureSet>>,
    pub edit_sources: Vec<VaMEditSource>,

    pub morph_index: Arc<VaMMorphIndex>,

    pub package_index: Arc<PackageIndex>,

    pub morph_groups: Vec<String>,
    pub morph_regions: Vec<String>,
    pub uv_mapping: Option<Arc<G2UvMapping>>,
    pub appearance_warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct VaMMorphCatalogPayload {
    pub cached_morphs: Vec<CachedBuiltinMorph>,

    pub canonical_faces: Arc<Vec<Vec<u32>>>,
    pub morph_cache_receipt: MorphCacheReceipt,
    pub morph_cache_warnings: Vec<String>,
}

#[derive(Debug)]
pub enum VaMWorkerEvent {
    AppearanceProgress {
        catalog_revision: u64,
        fraction: f32,
    },
    Appearance {
        catalog_revision: u64,
        outcome: Result<VaMCatalogPayload, String>,
    },
    MorphCatalog {
        catalog_revision: u64,
        outcome: Result<VaMMorphCatalogPayload, String>,
    },
    Morph {
        catalog_revision: u64,
        geometry_revision: u64,
        control_id: String,
        outcome: Result<VaMMorphPayload, String>,
    },
}

impl VaMWorkerEvent {
    const fn completes_request(&self) -> bool {
        !matches!(self, Self::AppearanceProgress { .. })
    }
}

#[derive(Debug)]
pub struct VaMCoordinator {
    sender: Sender<VaMWorkerEvent>,
    receiver: Receiver<VaMWorkerEvent>,
    active_catalog_revision: Option<u64>,
}

impl Default for VaMCoordinator {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            active_catalog_revision: None,
        }
    }
}

impl VaMCoordinator {
    pub fn start(
        &mut self,
        request: VaMWorkRequest,
        wake: impl Fn() + Send + 'static,
    ) -> Result<(), String> {
        if self.active_catalog_revision.is_some() {
            return Err("an asset catalog worker is already active".to_owned());
        }
        let catalog_revision = request.catalog_revision();
        let thread_name = request.thread_name();
        let fallback = request.clone();
        let sender = self.sender.clone();
        thread::Builder::new()
            .name(thread_name.to_owned())
            .spawn(move || {
                let event = catch_unwind(AssertUnwindSafe(|| {
                    execute_with_progress(request, &sender, &wake)
                }))
                .unwrap_or_else(|payload| fallback.failure_event(panic_detail(payload)));
                let _ = sender.send(event);
                wake();
            })
            .map_err(|error| format!("failed to spawn the asset catalog worker: {error}"))?;
        self.active_catalog_revision = Some(catalog_revision);
        Ok(())
    }

    pub fn drain(&mut self) -> Vec<VaMWorkerEvent> {
        let events: Vec<_> = self.receiver.try_iter().collect();
        if events.iter().any(VaMWorkerEvent::completes_request) {
            self.active_catalog_revision = None;
        }
        events
    }

    pub const fn is_active(&self) -> bool {
        self.active_catalog_revision.is_some()
    }
}

fn execute_with_progress(
    request: VaMWorkRequest,
    sender: &Sender<VaMWorkerEvent>,
    wake: &impl Fn(),
) -> VaMWorkerEvent {
    match request {
        VaMWorkRequest::ScanAppearance {
            catalog_revision,
            requested_root,
            figure_sex,
            template_geometry,
            force_rescan,
        } => {
            let outcome = (|| {
                report_appearance_progress(sender, wake, catalog_revision, 0.02);
                let root = VaMRoot::open(&requested_root).map_err(|error| error.to_string())?;
                report_appearance_progress(sender, wake, catalog_revision, 0.08);
                let mut appearance_warnings = Vec::new();

                let scan_started = std::time::Instant::now();
                let fingerprint = appearance_sources_fingerprint(root.path());
                let fingerprint_ms = scan_started.elapsed().as_millis();
                report_appearance_progress(sender, wake, catalog_revision, 0.12);
                let cached = if force_rescan {
                    None
                } else {
                    load_appearance_cache(root.path(), &fingerprint)
                };
                let from_cache = cached.is_some();
                let (skin_presets, hair_presets, shared_scalp) = if let Some(cache) = cached {
                    appearance_warnings.extend(cache.warnings);
                    (cache.skin_presets, cache.hair_presets, cache.shared_scalp)
                } else {
                    let skin_presets = match scan_skin_library_with_report(&root) {
                        Ok(report) => {
                            appearance_warnings.extend(report.diagnostics.warnings);
                            report.presets
                        }
                        Err(error) => {
                            appearance_warnings.push(format!("skin library: {error}"));
                            Vec::new()
                        }
                    };
                    let (hair_presets, shared_scalp) = match scan_hair_presets_with_report(&root) {
                        Ok(report) => {
                            appearance_warnings.extend(report.diagnostics.warnings);
                            (report.presets, report.shared_scalp.map(Box::new))
                        }
                        Err(error) => {
                            appearance_warnings.push(format!("hair library: {error}"));
                            (Vec::new(), None)
                        }
                    };
                    (skin_presets, hair_presets, shared_scalp)
                };
                if !from_cache {
                    store_appearance_cache(
                        root.path(),
                        &fingerprint,
                        &skin_presets,
                        &hair_presets,
                        shared_scalp.as_deref(),
                        &appearance_warnings,
                    );
                }
                let library_ms = scan_started.elapsed().as_millis() - fingerprint_ms;
                let scalps_started = std::time::Instant::now();
                let builtin_hair_scalps = match load_builtin_hair_scalps_cached(&root) {
                    Ok(scalps) => Arc::new(scalps),
                    Err(error) => {
                        appearance_warnings.push(format!("built-in hair scalps: {error}"));
                        Arc::new(Vec::new())
                    }
                };
                let builtin_scalp_textures = Arc::new(vkit_core::vam::scan_builtin_scalp_textures(
                    &root,
                    vkit_core::cache_root().as_deref(),
                ));
                let scalps_ms = scalps_started.elapsed().as_millis();
                report_appearance_progress(sender, wake, catalog_revision, 0.55);
                report_appearance_progress(sender, wake, catalog_revision, 0.60);
                let edit_sources_started = std::time::Instant::now();
                let (edit_sources, morph_groups, morph_regions) =
                    match scan_edit_sources(root.path()) {
                        Ok(report) => {
                            appearance_warnings.extend(report.warnings);
                            (report.sources, report.groups, report.regions)
                        }
                        Err(error) => {
                            appearance_warnings.push(format!("edit sources: {error}"));
                            (Vec::new(), Vec::new(), Vec::new())
                        }
                    };
                let edit_sources_ms = edit_sources_started.elapsed().as_millis();
                report_appearance_progress(sender, wake, catalog_revision, 0.84);

                let morphs_started = std::time::Instant::now();
                let morph_index = Arc::new(VaMMorphIndex::load_or_build(root.path(), force_rescan));
                let morphs_ms = morphs_started.elapsed().as_millis();
                report_appearance_progress(sender, wake, catalog_revision, 0.88);
                let packages_started = std::time::Instant::now();
                let package_index = Arc::new(load_or_build_package_index(
                    &root,
                    force_rescan,
                    &mut appearance_warnings,
                    |fraction| {
                        report_appearance_progress(
                            sender,
                            wake,
                            catalog_revision,
                            0.88 + fraction * 0.04,
                        );
                    },
                ));
                let _ = crate::diagnostics::record(
                    crate::diagnostics::Severity::Info,
                    "catalog",
                    "appearance_scan_timing",
                    &format!(
                        "from_cache={from_cache}; fingerprint_ms={fingerprint_ms}; library_ms={library_ms}; scalps_ms={scalps_ms}; edit_sources_ms={edit_sources_ms}; morphs_ms={morphs_ms}; packages_ms={}; total_ms={}",
                        packages_started.elapsed().as_millis(),
                        scan_started.elapsed().as_millis(),
                    ),
                );
                report_appearance_progress(sender, wake, catalog_revision, 0.92);
                let uv_mapping = template_geometry.as_deref().and_then(|geometry| {
                    match load_g2_uv_mapping_for_sex(&root, geometry, figure_sex) {
                        Ok(mapping) => Some(Arc::new(mapping)),
                        Err(error) => {
                            appearance_warnings.push(format!("G2 UV mapping: {error}"));
                            None
                        }
                    }
                });
                report_appearance_progress(sender, wake, catalog_revision, 0.97);
                Ok(VaMCatalogPayload {
                    canonical_root: root.path().to_path_buf(),
                    skin_presets,
                    hair_presets,
                    shared_scalp,
                    builtin_hair_scalps,
                    builtin_scalp_textures,
                    edit_sources,
                    morph_index,
                    package_index,
                    morph_groups,
                    morph_regions,
                    uv_mapping,
                    appearance_warnings,
                })
            })();
            VaMWorkerEvent::Appearance {
                catalog_revision,
                outcome,
            }
        }
        VaMWorkRequest::LoadMorphCache {
            catalog_revision,
            figure_sex,
            template_geometry,
            vam_root,
        } => {
            let outcome = (|| {
                let canonical_faces = Arc::new(template_geometry.faces.clone());

                let root = VaMRoot::open(&vam_root).map_err(|error| error.to_string())?;

                let cache_root =
                    crate::vam_morph_cache::default_cache_root().unwrap_or_else(std::env::temp_dir);
                let result = crate::vam_morph_cache::load_or_build(
                    &root,
                    figure_sex,
                    template_geometry.as_ref(),
                    &cache_root,
                )?;
                Ok(VaMMorphCatalogPayload {
                    cached_morphs: result.morphs,
                    canonical_faces,
                    morph_cache_receipt: result.receipt,
                    morph_cache_warnings: result.warnings,
                })
            })();
            VaMWorkerEvent::MorphCatalog {
                catalog_revision,
                outcome,
            }
        }
        VaMWorkRequest::ResolveCachedMorph {
            catalog_revision,
            geometry_revision,
            control_id,
            descriptor,
            vertex_count,
            canonical_faces,
        } => {
            let outcome = MorphTarget::from_sparse_deltas_shared(
                format!("builtin:{}", descriptor.internal_name),
                vertex_count,
                descriptor.deltas.as_slice(),
                canonical_faces,
                MorphAuthoring::vam(descriptor.minimum, descriptor.maximum, descriptor.default),
            )
            .map(|target| VaMMorphPayload {
                target: Arc::new(target),
                unsupported_formula_count: descriptor.unsupported_formula_count,
            })
            .map_err(|error| error.to_string());
            VaMWorkerEvent::Morph {
                catalog_revision,
                geometry_revision,
                control_id,
                outcome,
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
struct AppearanceCache {
    version: u32,
    fingerprint: String,
    skin_presets: Vec<SkinPreset>,
    hair_presets: Vec<HairPreset>,
    #[serde(default)]
    shared_scalp: Option<Box<HairPartReference>>,
    warnings: Vec<String>,
}

fn load_or_build_package_index(
    root: &VaMRoot,
    force_rescan: bool,
    warnings: &mut Vec<String>,
    mut progress: impl FnMut(f32),
) -> PackageIndex {
    let cache = package_index_cache_path(root.path());
    let previous = if force_rescan {
        None
    } else {
        cache.as_deref().and_then(PackageIndex::read_cache)
    };
    let index = PackageIndex::build(&root.addon_packages_path(), previous.as_ref(), |report| {
        if report.total > 0 {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a progress fraction over a package count"
            )]
            progress(report.done as f32 / report.total as f32);
        }
    });
    if !index.unreadable.is_empty() {
        let named = index
            .unreadable
            .iter()
            .take(3)
            .map(|path| {
                path.file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>()
            .join(", ");
        let extra = index.unreadable.len().saturating_sub(3);
        warnings.push(if extra > 0 {
            format!("unreadable packages: {named} (+{extra})")
        } else {
            format!("unreadable packages: {named}")
        });
    }
    if let Some(path) = cache.as_deref()
        && let Err(error) = index.write_cache(path)
    {
        warnings.push(format!("package index cache: {error}"));
    }
    index
}

fn package_index_cache_path(root: &Path) -> Option<PathBuf> {
    if cfg!(test) {
        return None;
    }
    let key = crate::cache_paths::cache_key_for_root(root);
    Some(
        vkit_core::cache_root()?
            .join("packages")
            .join(format!("index-{key}.json")),
    )
}

const APPEARANCE_CACHE_VERSION: u32 = 4;

fn appearance_cache_path(root: &Path) -> Option<PathBuf> {
    if cfg!(test) {
        return None;
    }
    let key = crate::cache_paths::cache_key_for_root(root);
    Some(
        vkit_core::cache_root()?
            .join("appearance")
            .join(format!("catalog-{key}.json")),
    )
}

fn load_builtin_hair_scalps_cached(
    root: &vkit_core::vam::VaMRoot,
) -> Result<Vec<vkit_core::vam::BuiltinHairScalp>, String> {
    #[derive(serde::Serialize, serde::Deserialize)]
    struct ScalpCache {
        version: u32,
        bundle_len: u64,
        bundle_mtime: u64,
        scalps: Vec<vkit_core::vam::BuiltinHairScalp>,
    }
    const SCALP_CACHE_VERSION: u32 = 1;

    let bundle = root
        .path()
        .join("VaM_Data")
        .join("StreamingAssets")
        .join("a_per");
    let stamp = fs::metadata(&bundle).ok().map(|meta| {
        (
            meta.len(),
            meta.modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |value| value.as_secs()),
        )
    });
    let cache_path = (!cfg!(test))
        .then(|| {
            vkit_core::cache_root().map(|base| {
                base.join("hair-scalps").join(format!(
                    "scalps-{}.json",
                    crate::cache_paths::cache_key_for_root(root.path())
                ))
            })
        })
        .flatten();
    if let (Some(path), Some((len, mtime))) = (cache_path.as_deref(), stamp)
        && let Some(cache) = fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<ScalpCache>(&bytes).ok())
        && cache.version == SCALP_CACHE_VERSION
        && cache.bundle_len == len
        && cache.bundle_mtime == mtime
    {
        return Ok(cache.scalps);
    }
    let scalps = load_builtin_hair_scalps(root).map_err(|error| error.to_string())?;
    if let (Some(path), Some((len, mtime))) = (cache_path.as_deref(), stamp) {
        let cache = ScalpCache {
            version: SCALP_CACHE_VERSION,
            bundle_len: len,
            bundle_mtime: mtime,
            scalps: scalps.clone(),
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(bytes) = serde_json::to_vec(&cache) {
            let _ = fs::write(path, bytes);
        }
    }
    Ok(scalps)
}

fn appearance_sources_fingerprint(root: &Path) -> String {
    fn visit(directory: &Path, extensions: &[&str], entries: &mut Vec<String>, depth: usize) {
        if depth > 8 {
            return;
        }
        let Ok(listing) = fs::read_dir(directory) else {
            return;
        };
        for entry in listing.flatten() {
            let path = entry.path();
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                visit(&path, extensions, entries, depth + 1);
                continue;
            }
            let matches = extensions.iter().any(|extension| {
                path.extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| value.eq_ignore_ascii_case(extension))
            });
            if !matches {
                continue;
            }
            let modified = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |value| value.as_secs());
            entries.push(format!(
                "{}|{}|{}",
                path.to_string_lossy().to_lowercase(),
                metadata.len(),
                modified
            ));
        }
    }

    let person = root.join("Custom").join("Atom").join("Person");
    let mut entries = Vec::new();
    visit(&person.join("Skin"), &["vap"], &mut entries, 0);
    visit(&person.join("Appearance"), &["vap"], &mut entries, 0);
    visit(&person.join("Hair"), &["vap"], &mut entries, 0);
    visit(&root.join("AddonPackages"), &["var"], &mut entries, 0);
    visit(
        &root.join("VaM_Data").join("StreamingAssets"),
        &["", "assets"],
        &mut entries,
        0,
    );
    entries.sort_unstable();
    let mut hasher = Sha256::new();
    for entry in &entries {
        hasher.update(entry.as_bytes());
        hasher.update([0]);
    }
    format!(
        "{}-{}",
        entries.len(),
        crate::cache_paths::hex_prefix(&hasher.finalize(), 16)
    )
}

fn load_appearance_cache(root: &Path, fingerprint: &str) -> Option<AppearanceCache> {
    let path = appearance_cache_path(root)?;
    let bytes = fs::read(path).ok()?;
    let cache: AppearanceCache = serde_json::from_slice(&bytes).ok()?;
    (cache.version == APPEARANCE_CACHE_VERSION && cache.fingerprint == fingerprint).then_some(cache)
}

fn store_appearance_cache(
    root: &Path,
    fingerprint: &str,
    skin_presets: &[SkinPreset],
    hair_presets: &[HairPreset],
    shared_scalp: Option<&HairPartReference>,
    warnings: &[String],
) {
    let Some(path) = appearance_cache_path(root) else {
        return;
    };
    let cache = AppearanceCache {
        version: APPEARANCE_CACHE_VERSION,
        fingerprint: fingerprint.to_owned(),
        skin_presets: skin_presets.to_vec(),
        hair_presets: hair_presets.to_vec(),
        shared_scalp: shared_scalp.cloned().map(Box::new),
        warnings: warnings.to_vec(),
    };
    let Ok(encoded) = serde_json::to_vec(&cache) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let staged = path.with_extension("json.tmp");
    if fs::write(&staged, &encoded).is_ok() {
        let _ = fs::rename(&staged, &path);
    }
}

#[cfg(test)]
fn execute(request: VaMWorkRequest) -> VaMWorkerEvent {
    let (sender, _receiver) = mpsc::channel();
    execute_with_progress(request, &sender, &|| {})
}

fn report_appearance_progress(
    sender: &Sender<VaMWorkerEvent>,
    wake: &impl Fn(),
    catalog_revision: u64,
    fraction: f32,
) {
    let _ = sender.send(VaMWorkerEvent::AppearanceProgress {
        catalog_revision,
        fraction,
    });
    wake();
}

fn panic_detail(payload: Box<dyn Any + Send>) -> String {
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("unknown panic payload");
    format!("asset catalog worker stopped unexpectedly: {message}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tiny_geometry() -> DazGeometry {
        DazGeometry::new(
            "cache-policy-fixture".into(),
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![vec![0, 1, 2]],
            vkit_core::formats::GroupTable {
                indices: vec![0],
                names: vec!["fixture".into()],
            },
            vkit_core::formats::GroupTable {
                indices: vec![0],
                names: vec!["Face".into()],
            },
            json!({}),
        )
        .unwrap()
    }

    #[test]
    fn missing_root_finishes_as_an_error_without_panicking() {
        let mut coordinator = VaMCoordinator::default();
        let root = std::env::temp_dir().join(format!("vkit-missing-vam-{}", std::process::id()));
        coordinator
            .start(
                VaMWorkRequest::ScanAppearance {
                    catalog_revision: 7,
                    requested_root: root,
                    figure_sex: SkinSex::Female,
                    template_geometry: None,
                    force_rescan: true,
                },
                || {},
            )
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let events = coordinator.drain();
            if let Some((catalog_revision, outcome)) = events.into_iter().find_map(|event| {
                let VaMWorkerEvent::Appearance {
                    catalog_revision,
                    outcome,
                } = event
                else {
                    return None;
                };
                Some((catalog_revision, outcome))
            }) {
                assert_eq!(catalog_revision, 7);
                assert!(outcome.is_err());
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
    }

    #[test]
    fn skin_catalog_does_not_require_or_decode_a_morph_bank() {
        let root = std::env::temp_dir().join(format!(
            "vkit-vam-skin-with-invalid-bank-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let appearance = root
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Appearance");
        let texture = root
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Textures")
            .join("Test")
            .join("faceD.png");
        let bank = root.join("VaM_Data").join("StreamingAssets").join("f_mb");
        std::fs::create_dir_all(&appearance).unwrap();
        std::fs::create_dir_all(texture.parent().unwrap()).unwrap();
        std::fs::create_dir_all(bank.parent().unwrap()).unwrap();
        std::fs::write(&texture, b"image").unwrap();

        std::fs::write(&bank, b"not a morph bank").unwrap();
        std::fs::write(
            appearance.join("SkinOnly.vap"),
            br#"{"storables":[{"id":"geometry","character":"Female Custom"},{"id":"textures","faceDiffuseUrl":"Custom/Atom/Person/Textures/Test/faceD.png"}]}"#,
        )
        .unwrap();

        let event = execute(VaMWorkRequest::ScanAppearance {
            catalog_revision: 31,
            requested_root: root.clone(),
            figure_sex: SkinSex::Female,
            template_geometry: None,
            force_rescan: true,
        });
        let VaMWorkerEvent::Appearance {
            catalog_revision,
            outcome,
        } = event
        else {
            panic!("expected appearance catalog completion");
        };
        assert_eq!(catalog_revision, 31);
        let payload = outcome.unwrap();
        assert_eq!(payload.skin_presets.len(), 1);
        assert_eq!(payload.skin_presets[0].sex, SkinSex::Female);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn embedded_sparse_morph_resolves_without_a_vam_root_or_bank_session() {
        let descriptor = CachedBuiltinMorph {
            internal_name: "PHMCacheOnly".into(),
            label: "Cache Only".into(),
            category: vkit_core::vam::MorphCategory::Mouth,
            minimum: 0.0,
            maximum: 1.0,
            default: 0.0,
            deltas: Arc::new(vec![(1, [0.0, 0.25, 0.0])]),
            unsupported_formula_count: 3,
        };
        let canonical_faces = Arc::new(vec![vec![0, 1, 2]]);
        let event = execute(VaMWorkRequest::ResolveCachedMorph {
            catalog_revision: 4,
            geometry_revision: 9,
            control_id: "PHMCacheOnly".into(),
            descriptor: Box::new(descriptor),
            vertex_count: 3,
            canonical_faces: Arc::clone(&canonical_faces),
        });
        let VaMWorkerEvent::Morph {
            catalog_revision,
            geometry_revision,
            control_id,
            outcome,
        } = event
        else {
            panic!("expected cached morph completion");
        };
        assert_eq!(catalog_revision, 4);
        assert_eq!(geometry_revision, 9);
        assert_eq!(control_id, "PHMCacheOnly");
        let payload = outcome.unwrap();
        assert_eq!(Arc::strong_count(&canonical_faces), 2);
        assert_eq!(payload.target.deltas[1], [0.0, 0.25, 0.0]);
        assert_eq!(payload.unsupported_formula_count, 3);
    }

    #[test]
    fn a_root_that_is_not_an_installation_fails_the_morph_lane_cleanly() {
        let event = execute(VaMWorkRequest::LoadMorphCache {
            catalog_revision: 44,
            figure_sex: SkinSex::Female,
            template_geometry: Arc::new(tiny_geometry()),
            vam_root: std::env::temp_dir().join("vkit-not-a-vam-install"),
        });
        let VaMWorkerEvent::MorphCatalog {
            catalog_revision,
            outcome,
        } = event
        else {
            panic!("expected morph catalog completion");
        };
        assert_eq!(catalog_revision, 44);
        assert!(outcome.is_err());
    }
}
