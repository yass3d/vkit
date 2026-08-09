use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use flate2::read::DeflateDecoder;
use serde_json::Value;

use super::catalog::VaMRoot;
use super::{Result, VaMError, io_error};

const MAX_ARCHIVES: usize = 100_000;
const MAX_ARCHIVE_ENTRIES: usize = 1_000_000;
const MAX_CENTRAL_DIRECTORY_BYTES: usize = 512 * 1024 * 1024;
const MAX_PRESET_BYTES: usize = 16 * 1024 * 1024;
const MAX_PRESET_REFERENCE_DEPTH: usize = 8;
const MAX_PRESET_REFERENCE_BYTES: usize = 64 * 1024 * 1024;
const MAX_WALK_FILES: usize = 2_000_000;
const MAX_CACHE_TEXTURE_BYTES: u64 = 128 * 1024 * 1024;

const SCLERA_LACRIMAL_CACHE_PROFILES: &[&[&str]] =
    &[&["v5breeeyes8m"], &["eyeball", "color"], &["eyes2"]];

const DEFAULT_BUILTIN_IRIS_COLOR: u32 = 1;
const EYELASH_CACHE_PROFILES: &[&[&str]] =
    &[&["v5breelashes1"], &["eyelash", "alpha"], &["lashes"]];
const TEETH_CACHE_PROFILES: &[&[&str]] = &[&["teethdiffuse"], &["new", "teeth"], &["mouthd"]];
const GUMS_CACHE_PROFILES: &[&[&str]] = &[&["gumsd"], &["new", "gums", "tongue"], &["mouthd"]];
const MOUTH_CACHE_PROFILES: &[&[&str]] = &[&["mouthd"], &["mouth", "full", "decal", "main"]];
const TEETH_SPECULAR_CACHE_PROFILES: &[&[&str]] = &[&["teethspecular"]];
const TEETH_NORMAL_CACHE_PROFILES: &[&[&str]] = &[&["teethnormal"]];
const GUMS_SPECULAR_CACHE_PROFILES: &[&[&str]] = &[&["gumss"], &["mouth", "full", "spec"]];
const GUMS_NORMAL_CACHE_PROFILES: &[&[&str]] = &[&["gumsn"], &["mouth", "full", "normal"]];
const MOUTH_SPECULAR_CACHE_PROFILES: &[&[&str]] = &[&["mouth", "full", "spec"]];
const MOUTH_GLOSS_CACHE_PROFILES: &[&[&str]] = &[&["mouth", "full", "gloss"]];
const MOUTH_NORMAL_CACHE_PROFILES: &[&[&str]] = &[&["mouth", "full", "normal"]];

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AssetLocator {
    File(PathBuf),
    VarEntry { archive: PathBuf, entry: String },
    UnresolvedVaMUrl(String),

    BuiltinTexture(std::sync::Arc<super::unity_textures::BuiltinTextureRef>),
}

impl AssetLocator {
    pub fn display_key(&self) -> String {
        match self {
            Self::File(path) => path.display().to_string(),
            Self::VarEntry { archive, entry } => {
                format!("{}::{entry}", archive.display())
            }
            Self::UnresolvedVaMUrl(url) => url.clone(),
            Self::BuiltinTexture(reference) => format!(
                "{}::{}#{}",
                reference.bundle_path.display(),
                reference.cab_node,
                reference.texture_name
            ),
        }
    }

    pub fn read_bytes(&self, maximum_bytes: usize) -> Result<Vec<u8>> {
        if maximum_bytes == 0 {
            return Err(VaMError::InvalidSkinPreset {
                locator: self.display_key(),
                message: "asset byte limit must be positive".to_owned(),
            });
        }
        match self {
            Self::File(path) => {
                let size = fs::metadata(path)
                    .map_err(|error| io_error(path, error))?
                    .len();
                if size > maximum_bytes as u64 {
                    return Err(VaMError::InvalidSkinPreset {
                        locator: path.display().to_string(),
                        message: format!("asset contains {size} bytes; limit is {maximum_bytes}"),
                    });
                }
                fs::read(path).map_err(|error| io_error(path, error))
            }
            Self::VarEntry { archive, entry } => {
                let index = VarArchive::open(archive)?;
                index.read_entry(entry, maximum_bytes)
            }
            Self::UnresolvedVaMUrl(url) => Err(VaMError::InvalidSkinPreset {
                locator: url.clone(),
                message: "VaM package URL could not be resolved".to_owned(),
            }),
            Self::BuiltinTexture(_) => Err(VaMError::InvalidSkinPreset {
                locator: self.display_key(),
                message: "built-in bundle textures must be decoded, not read as raw bytes"
                    .to_owned(),
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, Serialize, Deserialize)]
pub enum SkinRegion {
    Face,
    Torso,
    Limbs,
    Genitals,
}

impl SkinRegion {
    pub const ALL: [Self; 4] = [Self::Face, Self::Torso, Self::Limbs, Self::Genitals];

    #[must_use]
    pub const fn vam_prefix(self) -> &'static str {
        match self {
            Self::Face => "face",
            Self::Torso => "torso",
            Self::Limbs => "limbs",
            Self::Genitals => "genitals",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, Serialize, Deserialize)]
pub enum SkinTextureChannel {
    Diffuse,
    Normal,
    Specular,
    Gloss,
    Detail,
    Decal,
}

impl SkinTextureChannel {
    pub const ALL: [Self; 6] = [
        Self::Diffuse,
        Self::Normal,
        Self::Specular,
        Self::Gloss,
        Self::Detail,
        Self::Decal,
    ];

    #[must_use]
    pub const fn vam_suffix(self) -> &'static str {
        match self {
            Self::Diffuse => "Diffuse",
            Self::Normal => "Normal",
            Self::Specular => "Specular",
            Self::Gloss => "Gloss",
            Self::Detail => "Detail",
            Self::Decal => "Decal",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkinTextureSet {
    entries: Vec<SkinTextureEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SkinTextureEntry {
    region: SkinRegion,
    channel: SkinTextureChannel,
    locator: AssetLocator,
}

impl SkinTextureSet {
    pub fn insert(
        &mut self,
        region: SkinRegion,
        channel: SkinTextureChannel,
        locator: AssetLocator,
    ) {
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|entry| entry.region == region && entry.channel == channel)
        {
            existing.locator = locator;
            return;
        }
        self.entries.push(SkinTextureEntry {
            region,
            channel,
            locator,
        });
    }

    #[must_use]
    pub fn get(&self, region: SkinRegion, channel: SkinTextureChannel) -> Option<&AssetLocator> {
        self.entries
            .iter()
            .find(|entry| entry.region == region && entry.channel == channel)
            .map(|entry| &entry.locator)
    }

    pub fn iter(&self) -> impl Iterator<Item = (SkinRegion, SkinTextureChannel, &AssetLocator)> {
        self.entries
            .iter()
            .map(|entry| (entry.region, entry.channel, &entry.locator))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn fill_missing_from(&mut self, fallback: &Self) {
        for (region, channel, locator) in fallback.iter() {
            if self.get(region, channel).is_none() {
                self.insert(region, channel, locator.clone());
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkinPreset {
    pub stable_id: String,
    pub label: String,
    pub source: AssetLocator,

    pub sex: SkinSex,

    pub textures: SkinTextureSet,

    pub auxiliary: SkinAuxiliary,
}

impl SkinPreset {
    #[must_use]
    pub fn diffuse(&self, region: SkinRegion) -> Option<&AssetLocator> {
        self.textures.get(region, SkinTextureChannel::Diffuse)
    }

    #[must_use]
    pub fn surface(&self, region: SkinRegion) -> SkinSurfaceLocators {
        SkinSurfaceLocators {
            normal: self
                .textures
                .get(region, SkinTextureChannel::Normal)
                .cloned(),
            specular: self
                .textures
                .get(region, SkinTextureChannel::Specular)
                .cloned(),
            gloss: self
                .textures
                .get(region, SkinTextureChannel::Gloss)
                .cloned(),
        }
    }
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum SkinSex {
    Female,
    Male,
    #[default]
    Unknown,
}

impl SkinSex {
    pub const fn is_compatible_with(self, target: Self, include_unknown: bool) -> bool {
        match (self, target) {
            (_, Self::Unknown) => true,
            (Self::Unknown, _) => include_unknown,
            (left, right) => left as u8 == right as u8,
        }
    }
}

pub fn filter_skin_presets_by_sex(
    presets: &[SkinPreset],
    target: SkinSex,
    include_unknown: bool,
) -> Vec<&SkinPreset> {
    presets
        .iter()
        .filter(|preset| preset.sex.is_compatible_with(target, include_unknown))
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkinAuxMaterial {
    pub diffuse: Option<AssetLocator>,

    pub diffuse_source: SkinDiffuseSource,
    pub surface: SkinSurfaceLocators,
    pub base_color: [u8; 4],
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkinDiffuseSource {
    #[default]
    None,

    CustomTexture,

    BuiltInSelectorResolved(String),

    BuiltInSelectorUnavailable(String),

    /// Supplied by this tool because the look named nothing for this region.
    FigureDefault,
}

impl SkinDiffuseSource {
    pub fn unresolved_builtin_selector(&self) -> Option<&str> {
        match self {
            Self::BuiltInSelectorUnavailable(selector) => Some(selector),
            _ => None,
        }
    }

    const fn allows_default_fallback(&self) -> bool {
        matches!(self, Self::None)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkinSurfaceLocators {
    pub normal: Option<AssetLocator>,
    pub specular: Option<AssetLocator>,
    pub gloss: Option<AssetLocator>,
}

impl SkinSurfaceLocators {
    fn fill_missing_from(&mut self, fallback: &Self) {
        if self.normal.is_none() {
            self.normal.clone_from(&fallback.normal);
        }
        if self.specular.is_none() {
            self.specular.clone_from(&fallback.specular);
        }
        if self.gloss.is_none() {
            self.gloss.clone_from(&fallback.gloss);
        }
    }
}

impl SkinAuxMaterial {
    const fn new(base_color: [u8; 4]) -> Self {
        Self {
            diffuse: None,
            diffuse_source: SkinDiffuseSource::None,
            surface: SkinSurfaceLocators {
                normal: None,
                specular: None,
                gloss: None,
            },
            base_color,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkinAuxiliary {
    pub sclera: SkinAuxMaterial,
    pub iris: SkinAuxMaterial,
    pub lacrimal: SkinAuxMaterial,
    pub inner_mouth: SkinAuxMaterial,
    pub teeth: SkinAuxMaterial,
    pub gums: SkinAuxMaterial,
    pub tongue: SkinAuxMaterial,

    pub eyelashes: SkinAuxMaterial,
}

impl Default for SkinAuxiliary {
    fn default() -> Self {
        Self {
            sclera: SkinAuxMaterial::new([238, 238, 228, 255]),
            iris: SkinAuxMaterial::new([82, 112, 116, 255]),
            lacrimal: SkinAuxMaterial::new([190, 92, 102, 255]),
            inner_mouth: SkinAuxMaterial::new([67, 18, 23, 255]),
            teeth: SkinAuxMaterial::new([242, 232, 205, 255]),
            gums: SkinAuxMaterial::new([151, 57, 68, 255]),
            tongue: SkinAuxMaterial::new([178, 75, 88, 255]),
            eyelashes: SkinAuxMaterial::new([22, 15, 11, 255]),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkinScanDiagnostics {
    pub skipped_archives: usize,
    pub skipped_presets: usize,
    pub warnings: Vec<String>,
}

impl SkinScanDiagnostics {
    fn record_archive(&mut self, locator: &str, error: &VaMError) {
        self.skipped_archives += 1;
        self.record_warning(format!("archive {locator}: {error}"));
    }

    fn record_preset(&mut self, locator: &str, error: &VaMError) {
        self.skipped_presets += 1;
        self.record_warning(format!("preset {locator}: {error}"));
    }

    fn record_warning(&mut self, warning: String) {
        if self.warnings.len() < 64 {
            self.warnings.push(warning);
            self.warnings.sort();
            self.warnings.dedup();
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkinScanReport {
    pub presets: Vec<SkinPreset>,
    pub diagnostics: SkinScanDiagnostics,
}

pub fn scan_skin_library(root: &VaMRoot) -> Result<Vec<SkinPreset>> {
    Ok(scan_skin_library_with_report(root)?.presets)
}

pub fn scan_skin_library_with_report(root: &VaMRoot) -> Result<SkinScanReport> {
    let mut diagnostics = SkinScanDiagnostics::default();
    let package_index = ScanPackages::scan(root, &mut diagnostics)?;
    let mut presets = BTreeMap::new();

    let person = root.person_assets_path();
    for branch in [person.join("Skin"), person.join("Appearance")] {
        for path in walk_files_if_exists(&branch)? {
            if has_extension(&path, "vap") {
                let source = AssetLocator::File(path.clone());
                let attempt = read_bounded_file(&path, MAX_PRESET_BYTES).and_then(|bytes| {
                    parse_vap(
                        &bytes,
                        source,
                        path.file_stem()
                            .and_then(|value| value.to_str())
                            .unwrap_or("Skin"),
                        PresetContainer::Local,
                        root,
                        &package_index,
                    )
                });
                match attempt {
                    Ok(Some(preset)) => {
                        presets.insert(preset.stable_id.clone(), preset);
                    }
                    Ok(None) => {}
                    Err(error) => diagnostics.record_preset(&path.display().to_string(), &error),
                }
            }
        }
    }

    let addon_root = root.addon_packages_path();
    for path in walk_files_if_exists(&addon_root)? {
        if has_extension(&path, "vap") && is_skin_or_appearance_path(&path) {
            let source = AssetLocator::File(path.clone());
            let container = package_index
                .expanded_owner(&path)
                .map(PresetContainer::Expanded)
                .unwrap_or(PresetContainer::Local);
            let attempt = read_bounded_file(&path, MAX_PRESET_BYTES).and_then(|bytes| {
                parse_vap(
                    &bytes,
                    source,
                    path.file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or("Skin"),
                    container,
                    root,
                    &package_index,
                )
            });
            match attempt {
                Ok(Some(preset)) => {
                    presets.insert(preset.stable_id.clone(), preset);
                }
                Ok(None) => {}
                Err(error) => diagnostics.record_preset(&path.display().to_string(), &error),
            }
        }
    }

    for (archive_index, archive) in package_index.archives.iter().enumerate() {
        for entry in &archive.entries {
            if !entry.name.to_ascii_lowercase().ends_with(".vap")
                || !is_skin_or_appearance_text(&entry.name)
            {
                continue;
            }
            let source = AssetLocator::VarEntry {
                archive: archive.path.clone(),
                entry: entry.name.clone(),
            };
            let label = Path::new(&entry.name)
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("Skin");
            let attempt = archive
                .read_entry(&entry.name, MAX_PRESET_BYTES)
                .and_then(|bytes| {
                    parse_vap(
                        &bytes,
                        source,
                        label,
                        PresetContainer::Archive(archive_index),
                        root,
                        &package_index,
                    )
                });
            match attempt {
                Ok(Some(preset)) => {
                    presets.insert(preset.stable_id.clone(), preset);
                }
                Ok(None) => {}
                Err(error) => diagnostics.record_preset(
                    &format!("{}::{}", archive.path.display(), entry.name),
                    &error,
                ),
            }
        }
    }

    // Scanned before the defaults are chosen, because the base figure's own
    // materials are those defaults.
    let builtin = match super::unity_textures::scan_builtin_skins(
        root,
        default_texture_cache_dir().as_deref(),
    ) {
        Ok((builtin, warnings)) => {
            for warning in warnings {
                diagnostics.record_warning(warning);
            }
            builtin
        }
        Err(error) => {
            diagnostics.record_warning(format!("built-in skin scan failed: {error}"));
            Vec::new()
        }
    };

    let defaults = discover_default_auxiliary(root, &builtin)?;
    for preset in presets.values_mut() {
        let fallback = defaults.for_sex(preset.sex);
        // Spread first so the look's own eye wins the regions it left silent,
        // then take stand-ins for everything still unnamed, then spread once
        // more: an iris the stand-in refused would otherwise be the one blank
        // island in an otherwise complete eye, which reads as a white eye.
        preset.auxiliary.spread_own_eye_atlas();
        preset
            .auxiliary
            .fill_missing_from(&fallback.auxiliary, fallback.iris_stands_in);
        preset.auxiliary.spread_own_eye_atlas();
        // A look that named a built-in eye colour is not warned about. VaM
        // paints those as an extra layer over the shared eye atlas, and this
        // tool shows the atlas underneath instead — a whole, ordinary eye,
        // just not the shade that look's author picked. That is a settled
        // decision, not a failure, and 160 looks produced twenty warnings a
        // scan about work that had gone correctly.
        //
        // `unavailable_builtin_selectors` is kept for the day that decision
        // changes: eyes and inner mouth are fixed here today because the tool
        // reshapes heads and reskins them, but if eye colour ever becomes
        // something a reader chooses, this is where the gap is visible again.
        // Resolving the colours themselves would mean reading Cache/Textures,
        // which is the runtime folder this release stopped depending on.
    }
    let mut result: Vec<_> = presets.into_values().collect();
    result.sort_by(|left, right| {
        (left.label.to_lowercase(), &left.stable_id)
            .cmp(&(right.label.to_lowercase(), &right.stable_id))
    });
    let mut combined = builtin;
    combined.extend(result);
    let result = combined;

    Ok(SkinScanReport {
        presets: result,
        diagnostics,
    })
}

fn default_texture_cache_dir() -> Option<PathBuf> {
    crate::cache_root()
}

impl SkinAuxiliary {
    pub fn unavailable_builtin_selectors(&self) -> Vec<(&'static str, &str)> {
        [
            ("sclera", &self.sclera),
            ("iris", &self.iris),
            ("lacrimal", &self.lacrimal),
            ("inner_mouth", &self.inner_mouth),
            ("teeth", &self.teeth),
            ("gums", &self.gums),
            ("tongue", &self.tongue),
            ("eyelashes", &self.eyelashes),
        ]
        .into_iter()
        .filter_map(|(region, material)| {
            material
                .diffuse_source
                .unresolved_builtin_selector()
                .map(|selector| (region, selector))
        })
        .collect()
    }

    /// Takes, for every region this look left unnamed, the stand-in's version.
    ///
    /// `iris_stands_in` says whether the stand-in iris is the figure's own
    /// default colour. A look that asked for a built-in colour this
    /// installation does not have may borrow that one, because it is what VaM
    /// would have shown underneath — but never some other colour, which would
    /// be a substitution the reader could only catch by eye.
    fn fill_missing_from(&mut self, fallback: &Self, iris_stands_in: bool) {
        for (region, target, fallback) in [
            ("sclera", &mut self.sclera, &fallback.sclera),
            ("iris", &mut self.iris, &fallback.iris),
            ("lacrimal", &mut self.lacrimal, &fallback.lacrimal),
            ("inner mouth", &mut self.inner_mouth, &fallback.inner_mouth),
            ("teeth", &mut self.teeth, &fallback.teeth),
            ("gums", &mut self.gums, &fallback.gums),
            ("tongue", &mut self.tongue, &fallback.tongue),
            ("eyelashes", &mut self.eyelashes, &fallback.eyelashes),
        ] {
            if target.diffuse.is_none() {
                if target.diffuse_source.allows_default_fallback() {
                    target.diffuse.clone_from(&fallback.diffuse);
                    target.diffuse_source.clone_from(&fallback.diffuse_source);
                } else if target
                    .diffuse_source
                    .unresolved_builtin_selector()
                    .is_some()
                    && (region != "iris" || iris_stands_in)
                {
                    target.diffuse.clone_from(&fallback.diffuse);
                }
            }
            target.surface.fill_missing_from(&fallback.surface);
        }
    }

    /// Puts this look's own eye image across all three eye regions.
    ///
    /// VaM paints sclera, iris and lacrimal from one atlas, and a look
    /// normally names it once — most often as the sclera. Spreading that one
    /// image over the regions it left silent is what keeps an eye whole:
    /// filling them from the stand-in instead would set a stranger's iris in
    /// the look's own eye. Regions the look did name are left alone, and
    /// regions no one named are left for the stand-in.
    fn spread_own_eye_atlas(&mut self) {
        let atlas = self
            .sclera
            .diffuse
            .as_ref()
            .or(self.iris.diffuse.as_ref())
            .or(self.lacrimal.diffuse.as_ref())
            .cloned();
        let Some(atlas) = atlas else {
            return;
        };
        for material in [&mut self.sclera, &mut self.iris, &mut self.lacrimal] {
            material.diffuse.get_or_insert_with(|| atlas.clone());
        }
    }

    /// Every region a look can name its own texture for.
    fn materials_mut(&mut self) -> [&mut SkinAuxMaterial; 8] {
        [
            &mut self.sclera,
            &mut self.iris,
            &mut self.lacrimal,
            &mut self.inner_mouth,
            &mut self.teeth,
            &mut self.gums,
            &mut self.tongue,
            &mut self.eyelashes,
        ]
    }
}

struct DefaultAuxiliary {
    female: SexDefaults,
    male: SexDefaults,
    unknown: SexDefaults,
}

/// One sex's stand-in eye, mouth and lash materials.
#[derive(Default)]
struct SexDefaults {
    auxiliary: SkinAuxiliary,

    /// Whether `auxiliary.iris` is the figure's own default colour, and so a
    /// safe stand-in for a look whose named colour is not installed.
    iris_stands_in: bool,
}

impl DefaultAuxiliary {
    const fn for_sex(&self, sex: SkinSex) -> &SexDefaults {
        match sex {
            SkinSex::Female => &self.female,
            SkinSex::Male => &self.male,
            SkinSex::Unknown => &self.unknown,
        }
    }
}

/// The materials a look inherits for the regions it names nothing for.
///
/// They come from the base figure's own material bundle, which is part of
/// every installation. `Cache/Textures` only fills what the bundle does not
/// carry: VaM writes that folder as it renders, so on an installation that
/// has not yet displayed these textures it is empty — and empty is exactly
/// what leaves eyes and inner mouth white.
fn discover_default_auxiliary(root: &VaMRoot, builtin: &[SkinPreset]) -> Result<DefaultAuxiliary> {
    let entries = cache_texture_entries(root)?;
    let for_sex = |sex: SkinSex| -> SexDefaults {
        let mut auxiliary = base_figure_auxiliary(builtin, sex);
        if !entries.is_empty() {
            auxiliary.fill_missing_from(&default_auxiliary_from_cache(&entries, sex), true);
        }
        let iris_stands_in = iris_is_the_default_color(auxiliary.iris.diffuse.as_ref());
        SexDefaults {
            auxiliary,
            iris_stands_in,
        }
    };
    Ok(DefaultAuxiliary {
        female: for_sex(SkinSex::Female),
        male: for_sex(SkinSex::Male),
        unknown: for_sex(SkinSex::Unknown),
    })
}

/// Whether an iris image is the figure's own default colour.
///
/// Only that one may stand in for a built-in colour this installation does
/// not have. Anything else — another look's iris, a differently numbered
/// preset — would be a silent substitution.
fn iris_is_the_default_color(diffuse: Option<&AssetLocator>) -> bool {
    match diffuse {
        // The base figure's own iris is its default colour by construction.
        Some(AssetLocator::BuiltinTexture(_)) => true,
        Some(AssetLocator::File(path)) => {
            builtin_iris_cache_color_number(path) == Some(DEFAULT_BUILTIN_IRIS_COLOR)
        }
        _ => false,
    }
}

fn base_figure_auxiliary(builtin: &[SkinPreset], sex: SkinSex) -> SkinAuxiliary {
    let wanted = super::unity_textures::builtin_stable_id(super::unity_textures::base_figure(sex));
    let mut auxiliary = builtin
        .iter()
        .find(|preset| preset.stable_id == wanted)
        .map(|preset| preset.auxiliary.clone())
        .unwrap_or_default();
    for material in auxiliary.materials_mut() {
        if material.diffuse.is_some() {
            material.diffuse_source = SkinDiffuseSource::FigureDefault;
        }
    }
    auxiliary
}

fn cache_texture_entries(root: &VaMRoot) -> Result<Vec<PathBuf>> {
    let cache = root.path().join("Cache").join("Textures");
    if !cache.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(&cache).map_err(|error| io_error(&cache, error))? {
        let entry = entry.map_err(|error| io_error(&cache, error))?;
        if !entry
            .file_type()
            .map_err(|error| io_error(entry.path(), error))?
            .is_file()
        {
            continue;
        }
        let path = entry.path();
        if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("vamcachemeta"))
            && valid_cache_pair(&path)
        {
            entries.push(path);
        }
    }
    entries.sort_by(|left, right| {
        cache_candidate_key(left)
            .cmp(&cache_candidate_key(right))
            .then_with(|| left.cmp(right))
    });
    Ok(entries)
}

fn default_auxiliary_from_cache(entries: &[PathBuf], sex: SkinSex) -> SkinAuxiliary {
    let mut auxiliary = SkinAuxiliary::default();
    let find =
        |profiles| find_cache_profile_for_sex(entries, profiles, sex).map(AssetLocator::File);
    let eye = find(SCLERA_LACRIMAL_CACHE_PROFILES);
    auxiliary.sclera.diffuse.clone_from(&eye);
    auxiliary.lacrimal.diffuse.clone_from(&eye);
    auxiliary.iris.diffuse = eye.or_else(|| {
        find_builtin_iris_cache(entries, DEFAULT_BUILTIN_IRIS_COLOR, sex).map(AssetLocator::File)
    });
    auxiliary.eyelashes.diffuse = find(EYELASH_CACHE_PROFILES);
    auxiliary.teeth.diffuse = find(TEETH_CACHE_PROFILES);
    auxiliary.teeth.surface.specular = find(TEETH_SPECULAR_CACHE_PROFILES);
    auxiliary.teeth.surface.normal = find(TEETH_NORMAL_CACHE_PROFILES);
    let gums = find(GUMS_CACHE_PROFILES);
    auxiliary.gums.diffuse.clone_from(&gums);
    auxiliary.tongue.diffuse = gums;
    let gums_specular = find(GUMS_SPECULAR_CACHE_PROFILES);
    auxiliary.gums.surface.specular.clone_from(&gums_specular);
    auxiliary.tongue.surface.specular = gums_specular;
    let gums_normal = find(GUMS_NORMAL_CACHE_PROFILES);
    auxiliary.gums.surface.normal.clone_from(&gums_normal);
    auxiliary.tongue.surface.normal = gums_normal;
    auxiliary.inner_mouth.diffuse = find(MOUTH_CACHE_PROFILES);
    auxiliary.inner_mouth.surface.specular = find(MOUTH_SPECULAR_CACHE_PROFILES);
    auxiliary.inner_mouth.surface.gloss = find(MOUTH_GLOSS_CACHE_PROFILES);
    auxiliary.inner_mouth.surface.normal = find(MOUTH_NORMAL_CACHE_PROFILES);
    for material in auxiliary.materials_mut() {
        if material.diffuse.is_some() {
            material.diffuse_source = SkinDiffuseSource::FigureDefault;
        }
    }
    auxiliary
}

fn valid_cache_pair(meta_path: &Path) -> bool {
    let data_path = meta_path.with_extension("vamcache");
    let Ok(metadata) = fs::metadata(&data_path) else {
        return false;
    };
    metadata.len() > 0 && metadata.len() <= MAX_CACHE_TEXTURE_BYTES
}

fn find_cache_profile_for_sex(
    entries: &[PathBuf],
    profiles: &[&[&str]],
    sex: SkinSex,
) -> Option<PathBuf> {
    for profile in profiles {
        let matches_profile = |path: &PathBuf| {
            let normalized = cache_candidate_key(path);
            profile.iter().all(|term| normalized.contains(term))
        };
        if let Some(candidate) = select_cache_candidate(entries, sex, matches_profile) {
            return Some(candidate.clone());
        }
    }
    None
}

fn select_cache_candidate(
    entries: &[PathBuf],
    sex: SkinSex,
    predicate: impl Fn(&PathBuf) -> bool,
) -> Option<&PathBuf> {
    if sex != SkinSex::Unknown
        && let Some(candidate) = entries
            .iter()
            .find(|path| predicate(path) && cache_candidate_sex(path) == sex)
    {
        return Some(candidate);
    }
    entries.iter().find(|path| {
        predicate(path)
            && (sex == SkinSex::Unknown || cache_candidate_sex(path) == SkinSex::Unknown)
    })
}

fn cache_candidate_sex(path: &Path) -> SkinSex {
    let key = cache_candidate_key(path);
    if key.contains("genesis2female") || key.contains("g2f") || key.contains("female") {
        SkinSex::Female
    } else if key.contains("genesis2male") || key.contains("g2m") || key.contains("male") {
        SkinSex::Male
    } else {
        SkinSex::Unknown
    }
}

fn cache_candidate_key(path: &Path) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[derive(Clone, Copy, Debug)]
enum PresetContainer {
    Local,
    Expanded(usize),
    Archive(usize),
}

fn parse_vap(
    bytes: &[u8],
    source: AssetLocator,
    label: &str,
    container: PresetContainer,
    root: &VaMRoot,
    packages: &ScanPackages,
) -> Result<Option<SkinPreset>> {
    let mut traversal = PresetTraversal::default();
    parse_vap_recursive(
        bytes,
        PresetVisit {
            source,
            label,
            container,
            root,
            packages,
            depth: 0,
        },
        &mut traversal,
    )
    .map(|parsed| parsed.map(|parsed| parsed.preset))
}

#[derive(Default)]
struct PresetTraversal {
    active_path: BTreeSet<String>,
    bytes_read: usize,
}

struct ParsedSkinPreset {
    preset: SkinPreset,
    sex_claims: SkinSexClaims,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SkinSexClaims(u8);

impl SkinSexClaims {
    const FEMALE: u8 = 1;
    const MALE: u8 = 2;

    fn record(&mut self, sex: SkinSex) {
        self.0 |= match sex {
            SkinSex::Female => Self::FEMALE,
            SkinSex::Male => Self::MALE,
            SkinSex::Unknown => 0,
        };
    }

    fn merge(&mut self, other: Self) {
        self.0 |= other.0;
    }

    const fn sex(self) -> SkinSex {
        match self.0 {
            Self::FEMALE => SkinSex::Female,
            Self::MALE => SkinSex::Male,
            _ => SkinSex::Unknown,
        }
    }
}

struct PresetVisit<'a> {
    source: AssetLocator,
    label: &'a str,
    container: PresetContainer,
    root: &'a VaMRoot,
    packages: &'a ScanPackages,
    depth: usize,
}

fn parse_vap_recursive(
    bytes: &[u8],
    visit: PresetVisit<'_>,
    traversal: &mut PresetTraversal,
) -> Result<Option<ParsedSkinPreset>> {
    let source_key = visit.source.display_key();
    let depth = visit.depth;
    if depth > MAX_PRESET_REFERENCE_DEPTH {
        return Err(VaMError::InvalidSkinPreset {
            locator: source_key,
            message: format!("nested preset reference depth exceeds {MAX_PRESET_REFERENCE_DEPTH}"),
        });
    }
    traversal.bytes_read = traversal
        .bytes_read
        .checked_add(bytes.len())
        .ok_or_else(|| VaMError::InvalidSkinPreset {
            locator: source_key.clone(),
            message: "nested preset byte count overflow".to_owned(),
        })?;
    if traversal.bytes_read > MAX_PRESET_REFERENCE_BYTES {
        return Err(VaMError::InvalidSkinPreset {
            locator: source_key,
            message: format!("nested preset data exceeds {MAX_PRESET_REFERENCE_BYTES} bytes"),
        });
    }
    let visit_key = normalized_id(&source_key);
    if !traversal.active_path.insert(visit_key.clone()) {
        return Err(VaMError::InvalidSkinPreset {
            locator: source_key,
            message: "cyclic nested skin preset reference".to_owned(),
        });
    }

    let parsed = parse_vap_recursive_body(bytes, visit, traversal);
    traversal.active_path.remove(&visit_key);
    parsed
}

fn parse_vap_recursive_body(
    bytes: &[u8],
    visit: PresetVisit<'_>,
    traversal: &mut PresetTraversal,
) -> Result<Option<ParsedSkinPreset>> {
    let PresetVisit {
        source,
        label,
        container,
        root,
        packages,
        depth,
    } = visit;
    let value: Value = crate::vam::simple_json::parse_document(bytes).map_err(|error| {
        VaMError::InvalidSkinPreset {
            locator: source.display_key(),
            message: error.to_string(),
        }
    })?;
    let storables = value
        .get("storables")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let textures = storables.iter().find(|item| {
        item.get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.eq_ignore_ascii_case("textures"))
    });

    let mut skin_textures = SkinTextureSet::default();
    for region in SkinRegion::ALL {
        for channel in SkinTextureChannel::ALL {
            let key = format!("{}{}Url", region.vam_prefix(), channel.vam_suffix());
            let Some(url) = textures
                .and_then(|textures| textures.get(&key))
                .and_then(Value::as_str)
                .filter(|value| is_texture_reference(value))
            else {
                continue;
            };
            skin_textures.insert(region, channel, packages.resolve_url(url, container, root));
        }
    }

    let mut parsed_auxiliary = parse_auxiliary_materials(storables, container, root, packages);
    let mut auxiliary = std::mem::take(&mut parsed_auxiliary.auxiliary);
    let mut inherited_sex = SkinSexClaims::default();
    for reference in person_preset_references(&value) {
        let locator = packages.resolve_url(&reference, container, root);
        if let AssetLocator::UnresolvedVaMUrl(unresolved) = &locator {
            return Err(VaMError::InvalidSkinPreset {
                locator: source.display_key(),
                message: format!("unresolved nested skin preset reference: {unresolved}"),
            });
        }
        let nested_bytes = locator.read_bytes(MAX_PRESET_BYTES)?;
        let nested_container = packages.container_for_locator(&locator);
        let nested_label = preset_label(&locator);
        let Some(nested) = parse_vap_recursive(
            &nested_bytes,
            PresetVisit {
                source: locator,
                label: &nested_label,
                container: nested_container,
                root,
                packages,
                depth: depth + 1,
            },
            traversal,
        )?
        else {
            continue;
        };
        let ParsedSkinPreset {
            preset: nested,
            sex_claims,
        } = nested;

        skin_textures.fill_missing_from(&nested.textures);
        auxiliary.fill_missing_from(&nested.auxiliary, false);
        inherited_sex.merge(sex_claims);
    }
    let face_diffuse = skin_textures
        .get(SkinRegion::Face, SkinTextureChannel::Diffuse)
        .cloned();
    let torso_diffuse = skin_textures
        .get(SkinRegion::Torso, SkinTextureChannel::Diffuse)
        .cloned();
    if face_diffuse.is_none() && torso_diffuse.is_none() {
        return Ok(None);
    }

    validate_required_texture("face diffuse", face_diffuse.as_ref(), &source, packages)?;
    validate_required_texture("torso diffuse", torso_diffuse.as_ref(), &source, packages)?;

    let mut sex_claims = classify_skin_sex_claims(
        &value,
        &source,
        face_diffuse.as_ref(),
        torso_diffuse.as_ref(),
    );
    sex_claims.merge(inherited_sex);
    let sex = sex_claims.sex();
    parsed_auxiliary.auxiliary = auxiliary;
    let auxiliary = parsed_auxiliary.finish(sex, &packages.cache_textures);
    let stable_id = format!("vam:skin:vap:{}", normalized_id(&source.display_key()));
    Ok(Some(ParsedSkinPreset {
        preset: SkinPreset {
            stable_id,
            label: label.to_owned(),
            source,
            sex,
            textures: skin_textures,
            auxiliary,
        },
        sex_claims,
    }))
}

fn validate_required_texture(
    role: &str,
    locator: Option<&AssetLocator>,
    preset_source: &AssetLocator,
    packages: &ScanPackages,
) -> Result<()> {
    let Some(locator) = locator else {
        return Ok(());
    };
    let readable = match locator {
        AssetLocator::BuiltinTexture(_) => true,
        AssetLocator::File(path) => {
            fs::metadata(path)
                .map(|metadata| {
                    metadata.is_file()
                        && metadata.len() > 0
                        && metadata.len() <= MAX_CACHE_TEXTURE_BYTES
                })
                .unwrap_or(false)
                && File::open(path).is_ok()
        }
        AssetLocator::VarEntry { archive, entry } => packages
            .archive_at(archive)
            .and_then(|archive| archive.entry_is_readable(entry, MAX_CACHE_TEXTURE_BYTES))
            .unwrap_or(false),
        AssetLocator::UnresolvedVaMUrl(_) => false,
    };
    if readable {
        Ok(())
    } else {
        Err(VaMError::InvalidSkinPreset {
            locator: preset_source.display_key(),
            message: format!(
                "{role} texture is unresolved or unreadable: {}",
                locator.display_key()
            ),
        })
    }
}

fn is_texture_reference(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && !value.to_ascii_lowercase().ends_with(".vap")
}

fn preset_label(locator: &AssetLocator) -> String {
    let path = match locator {
        AssetLocator::File(path) => path.as_path(),
        AssetLocator::VarEntry { entry, .. } => Path::new(entry),
        AssetLocator::UnresolvedVaMUrl(url) => Path::new(url),
        AssetLocator::BuiltinTexture(reference) => Path::new(reference.texture_name.as_str()),
    };
    path.file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Skin")
        .to_owned()
}

fn person_preset_references(value: &Value) -> Vec<String> {
    fn visit(value: &Value, result: &mut Vec<String>, seen: &mut BTreeSet<String>) {
        match value {
            Value::Array(values) => {
                for value in values {
                    visit(value, result, seen);
                }
            }
            Value::Object(values) => {
                for value in values.values() {
                    visit(value, result, seen);
                }
            }
            Value::String(value)
                if value.trim().to_ascii_lowercase().ends_with(".vap")
                    && is_skin_or_appearance_text(value) =>
            {
                let reference = value.trim().to_owned();
                if seen.insert(reference.to_ascii_lowercase()) {
                    result.push(reference);
                }
            }
            _ => {}
        }
    }

    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    visit(value, &mut result, &mut seen);
    result
}

#[cfg(test)]
fn classify_skin_sex(
    value: &Value,
    source: &AssetLocator,
    face: Option<&AssetLocator>,
    torso: Option<&AssetLocator>,
) -> SkinSex {
    classify_skin_sex_claims(value, source, face, torso).sex()
}

fn classify_skin_sex_claims(
    value: &Value,
    source: &AssetLocator,
    face: Option<&AssetLocator>,
    torso: Option<&AssetLocator>,
) -> SkinSexClaims {
    fn record_path(value: &str, claims: &mut SkinSexClaims) {
        let normalized = value.replace('\\', "/").to_ascii_lowercase();
        for segment in normalized.split(['/', ':']) {
            let token: String = segment
                .chars()
                .filter(|character| character.is_ascii_alphanumeric())
                .collect();
            if token.starts_with("female")
                || token.contains("genesis2female")
                || token.contains("g2fskin")
            {
                claims.record(SkinSex::Female);
            } else if token.starts_with("male")
                || token.contains("genesis2male")
                || token.contains("g2mskin")
            {
                claims.record(SkinSex::Male);
            }
        }
    }

    fn record_explicit(value: &str, claims: &mut SkinSexClaims) {
        let token: String = value
            .trim()
            .chars()
            .take_while(|character| character.is_ascii_alphabetic())
            .flat_map(char::to_lowercase)
            .collect();
        if token == "female" {
            claims.record(SkinSex::Female);
        } else if token == "male" {
            claims.record(SkinSex::Male);
        }
    }

    fn normalized_key(value: &str) -> String {
        value
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    }

    fn is_material_storable(id: &str) -> bool {
        matches!(
            normalized_key(id).as_str(),
            "skin"
                | "sclera"
                | "irises"
                | "iris"
                | "lacrimals"
                | "lacrimal"
                | "mouth"
                | "teeth"
                | "gums"
                | "tongue"
                | "femaleeyelashes"
                | "maleeyelashes"
                | "eyelashes"
        )
    }

    fn record_texture_storable(storable: &Value, claims: &mut SkinSexClaims) {
        let Some(values) = storable.as_object() else {
            return;
        };
        for (key, value) in values {
            let key = normalized_key(key);
            let relevant_region = ["face", "torso", "limbs", "genitals"]
                .iter()
                .any(|region| key.starts_with(region));
            if relevant_region
                && key.ends_with("url")
                && let Some(value) = value.as_str()
            {
                record_path(value, claims);
            }
        }
    }

    fn record_material_storable(storable: &Value, claims: &mut SkinSexClaims) {
        let Some(values) = storable.as_object() else {
            return;
        };
        for (key, value) in values {
            let key = normalized_key(key);
            if (key.starts_with("customtexture") || key.ends_with("url"))
                && let Some(value) = value.as_str()
            {
                record_path(value, claims);
            }
        }
    }

    fn record_geometry_morphs(storable: &Value, claims: &mut SkinSexClaims) {
        let Some(morphs) = storable.get("morphs").and_then(Value::as_array) else {
            return;
        };
        for morph in morphs {
            if let Some(uid) = morph.get("uid").and_then(Value::as_str) {
                record_path(uid, claims);
            }
        }
    }

    let mut claims = SkinSexClaims::default();
    let mut female_eyelashes = false;
    let mut male_eyelashes = false;
    let storables = value
        .get("storables")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    for storable in storables {
        let Some(id) = storable.get("id").and_then(Value::as_str) else {
            continue;
        };
        if id.eq_ignore_ascii_case("geometry") {
            if let Some(character) = storable.get("character").and_then(Value::as_str) {
                record_explicit(character, &mut claims);
            }
            record_geometry_morphs(storable, &mut claims);
        } else if id.eq_ignore_ascii_case("textures") {
            record_texture_storable(storable, &mut claims);
        } else if is_material_storable(id) {
            female_eyelashes |= id.eq_ignore_ascii_case("FemaleEyelashes");
            male_eyelashes |= id.eq_ignore_ascii_case("MaleEyelashes");
            record_material_storable(storable, &mut claims);
        }
    }
    if let Some(values) = value.as_object() {
        for key in ["gender", "sex", "figure"] {
            if let Some(value) = values.get(key).and_then(Value::as_str) {
                record_explicit(value, &mut claims);
            }
        }
    }

    if is_skin_or_appearance_text(&source.display_key()) {
        record_path(&source.display_key(), &mut claims);
    }
    if let Some(face) = face {
        record_path(&face.display_key(), &mut claims);
    }
    if let Some(torso) = torso {
        record_path(&torso.display_key(), &mut claims);
    }

    if claims.sex() == SkinSex::Unknown && claims.0 == 0 && female_eyelashes != male_eyelashes {
        claims.record(if female_eyelashes {
            SkinSex::Female
        } else {
            SkinSex::Male
        });
    }
    claims
}

#[derive(Clone, Copy)]
enum AuxiliarySelectorRegion {
    Sclera,
    Iris,
    Eyelashes,
}

struct EyelashChoice {
    material: SkinAuxMaterial,
    selector: Option<String>,
}

#[derive(Default)]
struct ParsedAuxiliaryMaterials {
    auxiliary: SkinAuxiliary,
    sclera_selector: Option<String>,
    iris_selector: Option<String>,
    female_eyelashes: Option<EyelashChoice>,
    male_eyelashes: Option<EyelashChoice>,
    generic_eyelashes: Option<EyelashChoice>,
}

impl ParsedAuxiliaryMaterials {
    fn finish(mut self, sex: SkinSex, cache_entries: &[PathBuf]) -> SkinAuxiliary {
        let eyelash = match sex {
            SkinSex::Female => self
                .female_eyelashes
                .take()
                .or_else(|| self.generic_eyelashes.take()),
            SkinSex::Male => self
                .male_eyelashes
                .take()
                .or_else(|| self.generic_eyelashes.take()),
            SkinSex::Unknown => self
                .generic_eyelashes
                .take()
                .or_else(|| self.female_eyelashes.take())
                .or_else(|| self.male_eyelashes.take()),
        };
        if let Some(choice) = eyelash {
            self.auxiliary.eyelashes = choice.material;
            fill_selector_diffuse(
                &mut self.auxiliary.eyelashes,
                choice.selector.as_deref(),
                AuxiliarySelectorRegion::Eyelashes,
                sex,
                cache_entries,
            );
        }
        fill_selector_diffuse(
            &mut self.auxiliary.sclera,
            self.sclera_selector.as_deref(),
            AuxiliarySelectorRegion::Sclera,
            sex,
            cache_entries,
        );

        if matches!(
            self.auxiliary.iris.diffuse,
            Some(AssetLocator::UnresolvedVaMUrl(_))
        ) {
            self.auxiliary.iris.diffuse = None;
            self.auxiliary.iris.diffuse_source = SkinDiffuseSource::None;
        }
        fill_selector_diffuse(
            &mut self.auxiliary.iris,
            self.iris_selector.as_deref(),
            AuxiliarySelectorRegion::Iris,
            sex,
            cache_entries,
        );
        self.auxiliary
    }
}

fn parse_auxiliary_materials(
    storables: &[Value],
    container: PresetContainer,
    root: &VaMRoot,
    packages: &ScanPackages,
) -> ParsedAuxiliaryMaterials {
    let mut parsed = ParsedAuxiliaryMaterials::default();
    for storable in storables {
        let Some(id) = storable.get("id").and_then(Value::as_str) else {
            continue;
        };
        if is_eyelash_material_id(id) {
            let mut material = SkinAuxiliary::default().eyelashes;
            apply_auxiliary_storable(&mut material, storable, true, container, root, packages);
            let selector = selector_value(storable, &["Eyelash", "Eyelashes"]);
            if material.diffuse.is_none()
                && let Some(selector) = selector.as_ref()
            {
                material.diffuse_source =
                    SkinDiffuseSource::BuiltInSelectorUnavailable(selector.clone());
            }
            let choice = EyelashChoice { material, selector };
            if id.eq_ignore_ascii_case("FemaleEyelashes") {
                parsed.female_eyelashes = Some(choice);
            } else if id.eq_ignore_ascii_case("MaleEyelashes") {
                parsed.male_eyelashes = Some(choice);
            } else {
                parsed.generic_eyelashes = Some(choice);
            }
            continue;
        }
        let selector = if id.eq_ignore_ascii_case("sclera") {
            selector_value(storable, &["Sclera"])
        } else if id.eq_ignore_ascii_case("irises") || id.eq_ignore_ascii_case("iris") {
            selector_value(storable, &["Irises", "Iris"])
        } else {
            None
        };
        let target = if id.eq_ignore_ascii_case("sclera") {
            parsed.sclera_selector.clone_from(&selector);
            Some(&mut parsed.auxiliary.sclera)
        } else if id.eq_ignore_ascii_case("irises") || id.eq_ignore_ascii_case("iris") {
            parsed.iris_selector.clone_from(&selector);
            Some(&mut parsed.auxiliary.iris)
        } else if id.eq_ignore_ascii_case("lacrimals") || id.eq_ignore_ascii_case("lacrimal") {
            Some(&mut parsed.auxiliary.lacrimal)
        } else if id.eq_ignore_ascii_case("mouth") {
            Some(&mut parsed.auxiliary.inner_mouth)
        } else if id.eq_ignore_ascii_case("teeth") {
            Some(&mut parsed.auxiliary.teeth)
        } else if id.eq_ignore_ascii_case("gums") {
            Some(&mut parsed.auxiliary.gums)
        } else if id.eq_ignore_ascii_case("tongue") {
            Some(&mut parsed.auxiliary.tongue)
        } else {
            None
        };
        let Some(target) = target else {
            continue;
        };
        apply_auxiliary_storable(target, storable, false, container, root, packages);
        if target.diffuse.is_none()
            && let Some(selector) = selector
        {
            target.diffuse_source = SkinDiffuseSource::BuiltInSelectorUnavailable(selector);
        }
    }
    parsed
}

fn apply_auxiliary_storable(
    target: &mut SkinAuxMaterial,
    storable: &Value,
    eyelash: bool,
    container: PresetContainer,
    root: &VaMRoot,
    packages: &ScanPackages,
) {
    if let Some(color) = storable.get("Diffuse Color").and_then(vam_hsv_color) {
        target.base_color = color;
    }
    let texture_key = if eyelash {
        "customTexture_AlphaTex"
    } else {
        "customTexture_MainTex"
    };
    target.diffuse = storable
        .get(texture_key)
        .and_then(Value::as_str)
        .filter(|url| !url.trim().is_empty())
        .map(|url| packages.resolve_url(url, container, root));
    if target.diffuse.is_some() {
        target.diffuse_source = SkinDiffuseSource::CustomTexture;
    }
    let resolve = |key: &str| {
        storable
            .get(key)
            .and_then(Value::as_str)
            .filter(|url| is_texture_reference(url))
            .map(|url| packages.resolve_url(url, container, root))
    };
    target.surface.normal = resolve("customTexture_BumpMap");
    target.surface.specular = resolve("customTexture_SpecTex");
    target.surface.gloss = resolve("customTexture_GlossTex");
}

fn selector_value(storable: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        storable
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

fn fill_selector_diffuse(
    material: &mut SkinAuxMaterial,
    selector: Option<&str>,
    region: AuxiliarySelectorRegion,
    sex: SkinSex,
    cache_entries: &[PathBuf],
) {
    if material.diffuse.is_some() {
        return;
    }
    let Some(selector) = selector
        .map(str::trim)
        .filter(|selector| !selector.is_empty())
    else {
        return;
    };
    if let Some(path) = find_selector_cache(cache_entries, region, selector, sex) {
        material.diffuse = Some(AssetLocator::File(path));
        material.diffuse_source = SkinDiffuseSource::BuiltInSelectorResolved(selector.to_owned());
    } else {
        material.diffuse_source =
            SkinDiffuseSource::BuiltInSelectorUnavailable(selector.to_owned());
    }
}

fn find_selector_cache(
    entries: &[PathBuf],
    region: AuxiliarySelectorRegion,
    selector: &str,
    sex: SkinSex,
) -> Option<PathBuf> {
    let selector = normalized_selector_key(selector);
    if selector.is_empty() {
        return None;
    }

    if matches!(region, AuxiliarySelectorRegion::Iris) {
        let color = builtin_iris_color_number(&selector)?;
        return find_builtin_iris_cache(entries, color, sex);
    }

    let region_terms: &[&str] = match region {
        AuxiliarySelectorRegion::Sclera => &["sclera"],
        AuxiliarySelectorRegion::Eyelashes => &["lash", "lashes", "eyelash"],
        AuxiliarySelectorRegion::Iris => unreachable!("iris handled above"),
    };
    select_cache_candidate(entries, sex, |path| {
        let key = cache_candidate_key(path);
        selector_key_occurs(&key, &selector) && region_terms.iter().any(|term| key.contains(term))
    })
    .cloned()
}

fn builtin_iris_color_number(selector: &str) -> Option<u32> {
    let digits = selector.strip_prefix("color")?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn find_builtin_iris_cache(entries: &[PathBuf], color: u32, sex: SkinSex) -> Option<PathBuf> {
    let matches = entries
        .iter()
        .filter(|path| builtin_iris_cache_color_number(path) == Some(color))
        .collect::<Vec<_>>();
    let preferred = if sex == SkinSex::Unknown {
        matches
            .into_iter()
            .filter(|path| cache_candidate_sex(path) == SkinSex::Unknown)
            .collect::<Vec<_>>()
    } else {
        let exact = matches
            .iter()
            .copied()
            .filter(|path| cache_candidate_sex(path) == sex)
            .collect::<Vec<_>>();
        if exact.is_empty() {
            matches
                .into_iter()
                .filter(|path| cache_candidate_sex(path) == SkinSex::Unknown)
                .collect()
        } else {
            exact
        }
    };

    (preferred.len() == 1).then(|| preferred[0].clone())
}

fn builtin_iris_cache_color_number(path: &Path) -> Option<u32> {
    let stem = path.file_stem()?.to_str()?.to_ascii_lowercase();
    let suffix = stem.strip_prefix("preset_dualcolor")?;
    let separator = suffix.find('_')?;
    let digits = &suffix[..separator];
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let image_kind = suffix[separator + 1..].split('_').next()?;
    if !matches!(
        image_kind,
        "jpg" | "jpeg" | "png" | "tga" | "tif" | "tiff" | "bmp" | "dds"
    ) {
        return None;
    }
    digits.parse().ok()
}

fn selector_key_occurs(candidate: &str, selector: &str) -> bool {
    let requires_numeric_end = selector.as_bytes().last().is_some_and(u8::is_ascii_digit);
    let mut start = 0;
    while let Some(relative) = candidate[start..].find(selector) {
        let end = start + relative + selector.len();
        if !requires_numeric_end
            || candidate
                .as_bytes()
                .get(end)
                .is_none_or(|next| !next.is_ascii_digit())
        {
            return true;
        }
        start = end;
    }
    false
}

fn normalized_selector_key(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_eyelash_material_id(id: &str) -> bool {
    id.eq_ignore_ascii_case("FemaleEyelashes")
        || id.eq_ignore_ascii_case("MaleEyelashes")
        || id.eq_ignore_ascii_case("Eyelashes")
}

fn vam_hsv_color(value: &Value) -> Option<[u8; 4]> {
    let parse = |key: &str| {
        value.get(key).and_then(|component| {
            component
                .as_f64()
                .or_else(|| component.as_str()?.parse().ok())
        })
    };
    let h = parse("h")?.rem_euclid(1.0);
    let s = parse("s")?.clamp(0.0, 1.0);
    let v = parse("v")?.clamp(0.0, 1.0);
    let sector = h * 6.0;
    let index = sector.floor() as u32;
    let fraction = sector - f64::from(index);
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * fraction);
    let t = v * (1.0 - s * (1.0 - fraction));
    let (r, g, b) = match index % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    let byte = |component: f64| (component * 255.0).round().clamp(0.0, 255.0) as u8;
    Some([byte(r), byte(g), byte(b), 255])
}

#[derive(Clone, Debug)]
struct ScanPackages {
    archives: Vec<VarArchive>,
    archive_by_uid: BTreeMap<String, Vec<usize>>,
    expanded: Vec<ExpandedPackage>,
    expanded_by_uid: BTreeMap<String, Vec<usize>>,
    cache_textures: Vec<PathBuf>,
}

impl ScanPackages {
    fn archive_at(&self, path: &Path) -> Option<&VarArchive> {
        self.archives.iter().find(|archive| archive.path == path)
    }
}

#[derive(Clone, Debug)]
struct ExpandedPackage {
    uid: String,
    path: PathBuf,
}

impl ScanPackages {
    fn scan(root: &VaMRoot, diagnostics: &mut SkinScanDiagnostics) -> Result<Self> {
        let addon_root = root.addon_packages_path();
        let mut archive_paths = Vec::new();
        let mut expanded = Vec::new();
        if addon_root.is_dir() {
            for entry in fs::read_dir(&addon_root).map_err(|error| io_error(&addon_root, error))? {
                let entry = entry.map_err(|error| io_error(&addon_root, error))?;
                let kind = entry
                    .file_type()
                    .map_err(|error| io_error(entry.path(), error))?;
                if kind.is_dir() {
                    expanded.push(ExpandedPackage {
                        uid: entry.file_name().to_string_lossy().into_owned(),
                        path: entry.path(),
                    });
                } else if kind.is_file() && has_extension(&entry.path(), "var") {
                    archive_paths.push(entry.path());
                }
            }
            for path in walk_files_if_exists(&addon_root)? {
                if has_extension(&path, "var") && !archive_paths.contains(&path) {
                    archive_paths.push(path);
                }
            }
        }
        archive_paths.sort_by_key(|path| path.to_string_lossy().to_ascii_lowercase());
        archive_paths.dedup();
        if archive_paths.len() > MAX_ARCHIVES {
            return Err(VaMError::InvalidSkinPreset {
                locator: addon_root.display().to_string(),
                message: format!("archive count exceeds {MAX_ARCHIVES}"),
            });
        }
        let mut archives = Vec::with_capacity(archive_paths.len());
        for path in archive_paths {
            match VarArchive::open(&path) {
                Ok(archive) => archives.push(archive),
                Err(error) => diagnostics.record_archive(&path.display().to_string(), &error),
            }
        }
        expanded.sort_by_key(|item| item.uid.to_ascii_lowercase());

        let mut archive_by_uid: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (index, archive) in archives.iter().enumerate() {
            archive_by_uid
                .entry(archive.uid.to_ascii_lowercase())
                .or_default()
                .push(index);
        }
        let mut expanded_by_uid: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (index, package) in expanded.iter().enumerate() {
            expanded_by_uid
                .entry(package.uid.to_ascii_lowercase())
                .or_default()
                .push(index);
        }
        Ok(Self {
            archives,
            archive_by_uid,
            expanded,
            expanded_by_uid,
            cache_textures: cache_texture_entries(root)?,
        })
    }

    fn expanded_owner(&self, path: &Path) -> Option<usize> {
        self.expanded
            .iter()
            .position(|package| path.starts_with(&package.path))
    }

    fn container_for_locator(&self, locator: &AssetLocator) -> PresetContainer {
        match locator {
            AssetLocator::BuiltinTexture(_) => PresetContainer::Local,
            AssetLocator::File(path) => self
                .expanded_owner(path)
                .map(PresetContainer::Expanded)
                .unwrap_or(PresetContainer::Local),
            AssetLocator::VarEntry { archive, .. } => self
                .archives
                .iter()
                .position(|candidate| candidate.path == *archive)
                .map(PresetContainer::Archive)
                .unwrap_or(PresetContainer::Local),
            AssetLocator::UnresolvedVaMUrl(_) => PresetContainer::Local,
        }
    }

    fn resolve_url(&self, raw: &str, owner: PresetContainer, root: &VaMRoot) -> AssetLocator {
        let url = raw.trim().replace('\\', "/");
        if is_windows_absolute_url(&url) || url.starts_with("//") {
            let direct = PathBuf::from(url.replace('/', "\\"));
            return if direct.is_file() {
                AssetLocator::File(direct)
            } else {
                AssetLocator::UnresolvedVaMUrl(url)
            };
        }
        if url
            .get(..6)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("self:/"))
        {
            let entry = &url[6..];
            return match owner {
                PresetContainer::Archive(index) => self
                    .archives
                    .get(index)
                    .and_then(|archive| archive.locator_for(entry))
                    .or_else(|| self.locate_unique_entry(entry))
                    .unwrap_or(AssetLocator::UnresolvedVaMUrl(url)),
                PresetContainer::Expanded(index) => self
                    .expanded
                    .get(index)
                    .map(|package| package.path.join(entry.replace('/', "\\")))
                    .filter(|path| path.is_file())
                    .map(AssetLocator::File)
                    .or_else(|| self.locate_unique_entry(entry))
                    .unwrap_or(AssetLocator::UnresolvedVaMUrl(url)),
                PresetContainer::Local => {
                    let path = root.path().join(entry.replace('/', "\\"));
                    if path.is_file() {
                        AssetLocator::File(path)
                    } else {
                        self.locate_unique_entry(entry)
                            .unwrap_or(AssetLocator::UnresolvedVaMUrl(url))
                    }
                }
            };
        }
        if let Some((uid, entry)) = url.split_once(":/") {
            if let Some(locator) = self.locate_package_entry(uid, entry) {
                return locator;
            }
            return AssetLocator::UnresolvedVaMUrl(url);
        }
        let direct = PathBuf::from(url.replace('/', "\\"));
        if direct.is_absolute() && direct.is_file() {
            return AssetLocator::File(direct);
        }
        let relative = root.path().join(url.replace('/', "\\"));
        if relative.is_file() {
            AssetLocator::File(relative)
        } else {
            AssetLocator::UnresolvedVaMUrl(url)
        }
    }

    fn locate_package_entry(&self, uid: &str, entry: &str) -> Option<AssetLocator> {
        let key = uid.to_ascii_lowercase();
        if let Some(expanded_index) = self.select_expanded(&key)
            && let Some(package) = self.expanded.get(expanded_index)
        {
            let path = package.path.join(entry.replace('/', "\\"));
            if path.is_file() {
                return Some(AssetLocator::File(path));
            }
        }
        self.select_archive(&key)
            .and_then(|archive_index| self.archives.get(archive_index))
            .and_then(|archive| archive.locator_for(entry))
    }

    fn locate_unique_entry(&self, entry: &str) -> Option<AssetLocator> {
        let expanded: Vec<_> = self
            .expanded
            .iter()
            .filter_map(|package| {
                let path = package.path.join(entry.replace('/', "\\"));
                path.is_file().then_some(AssetLocator::File(path))
            })
            .collect();
        match expanded.as_slice() {
            [locator] => return Some(locator.clone()),
            [] => {}
            _ => return None,
        }

        let archives: Vec<_> = self
            .archives
            .iter()
            .filter_map(|archive| archive.locator_for(entry))
            .collect();
        match archives.as_slice() {
            [locator] => Some(locator.clone()),
            _ => None,
        }
    }

    fn select_expanded(&self, key: &str) -> Option<usize> {
        select_package_index(key, &self.expanded_by_uid, |index| {
            &self.expanded[index].uid
        })
    }

    fn select_archive(&self, key: &str) -> Option<usize> {
        select_package_index(key, &self.archive_by_uid, |index| &self.archives[index].uid)
    }
}

fn is_windows_absolute_url(url: &str) -> bool {
    let bytes = url.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/'
}

fn select_package_index<T>(
    key: &str,
    exact: &BTreeMap<String, Vec<usize>>,
    uid: impl Fn(usize) -> T,
) -> Option<usize>
where
    T: AsRef<str>,
{
    if let Some(index) = exact.get(key).and_then(|values| values.first()) {
        return Some(*index);
    }
    let base = key.strip_suffix(".latest")?;
    exact
        .iter()
        .filter(|(candidate, _)| candidate.starts_with(&format!("{base}.")))
        .flat_map(|(_, values)| values.iter().copied())
        .max_by(|left, right| {
            version_key(uid(*left).as_ref()).cmp(&version_key(uid(*right).as_ref()))
        })
}

fn version_key(uid: &str) -> Vec<(bool, u64, String)> {
    uid.split('.')
        .map(|part| match part.parse::<u64>() {
            Ok(number) => (true, number, String::new()),
            Err(_) => (false, 0, part.to_ascii_lowercase()),
        })
        .collect()
}

pub fn read_var_entry_bytes(archive: &Path, entry: &str, maximum_bytes: usize) -> Result<Vec<u8>> {
    VarArchive::open(archive)?.read_entry(entry, maximum_bytes)
}

pub fn list_var_entries(archive: &Path) -> Result<Vec<String>> {
    Ok(VarArchive::open(archive)?
        .entries
        .into_iter()
        .map(|entry| entry.name)
        .collect())
}

#[derive(Clone, Debug)]
struct VarArchive {
    path: PathBuf,
    uid: String,
    entries: Vec<VarEntry>,
    lookup: BTreeMap<String, usize>,
}

#[derive(Clone, Debug)]
struct VarEntry {
    name: String,
    flags: u16,
    method: u16,
    compressed_size: u64,
    uncompressed_size: u64,
    local_header_offset: u64,
}

impl VarArchive {
    fn open(path: &Path) -> Result<Self> {
        let uid = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_owned();
        let entries = read_zip_directory(path)?;
        let mut lookup = BTreeMap::new();
        for (index, entry) in entries.iter().enumerate() {
            lookup
                .entry(normalized_archive_path(&entry.name))
                .or_insert(index);
        }
        Ok(Self {
            path: path.to_path_buf(),
            uid,
            entries,
            lookup,
        })
    }

    fn locator_for(&self, entry: &str) -> Option<AssetLocator> {
        let index = *self.lookup.get(&normalized_archive_path(entry))?;
        Some(AssetLocator::VarEntry {
            archive: self.path.clone(),
            entry: self.entries[index].name.clone(),
        })
    }

    fn read_entry(&self, entry: &str, maximum_bytes: usize) -> Result<Vec<u8>> {
        let index = self
            .lookup
            .get(&normalized_archive_path(entry))
            .copied()
            .ok_or_else(|| VaMError::InvalidSkinPreset {
                locator: format!("{}::{entry}", self.path.display()),
                message: "VAR entry does not exist".to_owned(),
            })?;
        read_zip_entry(&self.path, &self.entries[index], maximum_bytes)
    }

    fn entry_is_readable(&self, entry: &str, maximum_bytes: u64) -> Option<bool> {
        let index = self.lookup.get(&normalized_archive_path(entry)).copied()?;
        let entry = &self.entries[index];
        if entry.uncompressed_size == 0
            || entry.uncompressed_size > maximum_bytes
            || entry.flags & 1 != 0
            || !matches!(entry.method, 0 | 8)
        {
            return Some(false);
        }
        let mut file = File::open(&self.path).ok()?;
        file.seek(SeekFrom::Start(entry.local_header_offset)).ok()?;
        let mut header = [0_u8; 30];
        file.read_exact(&mut header).ok()?;
        Some(&header[0..4] == b"PK\x03\x04")
    }
}

fn read_zip_directory(path: &Path) -> Result<Vec<VarEntry>> {
    let mut file = File::open(path).map_err(|error| io_error(path, error))?;
    let size = file
        .metadata()
        .map_err(|error| io_error(path, error))?
        .len();
    let tail_size = size.min(65_557) as usize;
    file.seek(SeekFrom::End(-(tail_size as i64)))
        .map_err(|error| io_error(path, error))?;
    let mut tail = vec![0; tail_size];
    file.read_exact(&mut tail)
        .map_err(|error| io_error(path, error))?;
    let eocd = tail
        .windows(4)
        .rposition(|window| window == [0x50, 0x4b, 0x05, 0x06])
        .ok_or_else(|| VaMError::InvalidSkinPreset {
            locator: path.display().to_string(),
            message: "ZIP end-of-directory record was not found".to_owned(),
        })?;
    let footer = tail
        .get(eocd..)
        .ok_or_else(|| VaMError::InvalidSkinPreset {
            locator: path.display().to_string(),
            message: "ZIP footer is truncated".to_owned(),
        })?;
    if footer.len() < 22 {
        return Err(VaMError::InvalidSkinPreset {
            locator: path.display().to_string(),
            message: "ZIP footer is truncated".to_owned(),
        });
    }
    let entries = le_u16(footer, 10)? as usize;
    let directory_size = le_u32(footer, 12)? as usize;
    let directory_offset = le_u32(footer, 16)? as u64;
    if entries == u16::MAX as usize || directory_size == u32::MAX as usize {
        return Err(VaMError::UnsupportedEncoding(format!(
            "ZIP64 VAR archive {}",
            path.display()
        )));
    }
    if entries > MAX_ARCHIVE_ENTRIES || directory_size > MAX_CENTRAL_DIRECTORY_BYTES {
        return Err(VaMError::InvalidSkinPreset {
            locator: path.display().to_string(),
            message: "ZIP directory exceeds safety limits".to_owned(),
        });
    }
    file.seek(SeekFrom::Start(directory_offset))
        .map_err(|error| io_error(path, error))?;
    let mut directory = vec![0; directory_size];
    file.read_exact(&mut directory)
        .map_err(|error| io_error(path, error))?;
    let mut cursor = 0;
    let mut result = Vec::with_capacity(entries);
    for _ in 0..entries {
        let fixed =
            directory
                .get(cursor..cursor + 46)
                .ok_or_else(|| VaMError::InvalidSkinPreset {
                    locator: path.display().to_string(),
                    message: "ZIP central directory entry is truncated".to_owned(),
                })?;
        if &fixed[0..4] != b"PK\x01\x02" {
            return Err(VaMError::InvalidSkinPreset {
                locator: path.display().to_string(),
                message: "ZIP central directory signature is invalid".to_owned(),
            });
        }
        let name_len = le_u16(fixed, 28)? as usize;
        let extra_len = le_u16(fixed, 30)? as usize;
        let comment_len = le_u16(fixed, 32)? as usize;
        let variable_start = cursor + 46;
        let name_end =
            variable_start
                .checked_add(name_len)
                .ok_or_else(|| VaMError::InvalidSkinPreset {
                    locator: path.display().to_string(),
                    message: "ZIP filename length overflow".to_owned(),
                })?;
        let extra_end =
            name_end
                .checked_add(extra_len)
                .ok_or_else(|| VaMError::InvalidSkinPreset {
                    locator: path.display().to_string(),
                    message: "ZIP extra-field length overflow".to_owned(),
                })?;
        let name =
            String::from_utf8_lossy(directory.get(variable_start..name_end).ok_or_else(|| {
                VaMError::InvalidSkinPreset {
                    locator: path.display().to_string(),
                    message: "ZIP filename is truncated".to_owned(),
                }
            })?)
            .replace('\\', "/");
        let extra =
            directory
                .get(name_end..extra_end)
                .ok_or_else(|| VaMError::InvalidSkinPreset {
                    locator: path.display().to_string(),
                    message: "ZIP extra field is truncated".to_owned(),
                })?;
        let (uncompressed_size, compressed_size, local_header_offset) =
            central_zip64_sizes(fixed, extra)?;
        result.push(VarEntry {
            name,
            flags: le_u16(fixed, 8)?,
            method: le_u16(fixed, 10)?,
            compressed_size,
            uncompressed_size,
            local_header_offset,
        });
        cursor = extra_end
            .checked_add(comment_len)
            .ok_or_else(|| VaMError::InvalidSkinPreset {
                locator: path.display().to_string(),
                message: "ZIP central directory cursor overflow".to_owned(),
            })?;
    }
    result.sort_by_key(|entry| entry.name.to_ascii_lowercase());
    Ok(result)
}

fn read_zip_entry(path: &Path, entry: &VarEntry, maximum_bytes: usize) -> Result<Vec<u8>> {
    if entry.uncompressed_size > maximum_bytes as u64 {
        return Err(VaMError::InvalidSkinPreset {
            locator: format!("{}::{}", path.display(), entry.name),
            message: format!(
                "entry contains {} bytes; limit is {maximum_bytes}",
                entry.uncompressed_size
            ),
        });
    }
    if entry.flags & 1 != 0 {
        return Err(VaMError::UnsupportedEncoding(format!(
            "encrypted VAR entry {}::{}",
            path.display(),
            entry.name
        )));
    }
    let mut file = File::open(path).map_err(|error| io_error(path, error))?;
    file.seek(SeekFrom::Start(entry.local_header_offset))
        .map_err(|error| io_error(path, error))?;
    let mut header = [0_u8; 30];
    file.read_exact(&mut header)
        .map_err(|error| io_error(path, error))?;
    if &header[0..4] != b"PK\x03\x04" {
        return Err(VaMError::InvalidSkinPreset {
            locator: format!("{}::{}", path.display(), entry.name),
            message: "ZIP local header signature is invalid".to_owned(),
        });
    }
    let name_len = le_u16(&header, 26)? as u64;
    let extra_len = le_u16(&header, 28)? as u64;
    file.seek(SeekFrom::Current((name_len + extra_len) as i64))
        .map_err(|error| io_error(path, error))?;
    let compressed = file.take(entry.compressed_size);
    let mut output = Vec::with_capacity(entry.uncompressed_size as usize);
    match entry.method {
        0 => compressed
            .take(maximum_bytes as u64 + 1)
            .read_to_end(&mut output)
            .map_err(|error| io_error(path, error))?,
        8 => DeflateDecoder::new(compressed)
            .take(maximum_bytes as u64 + 1)
            .read_to_end(&mut output)
            .map_err(|error| io_error(path, error))?,
        method => {
            return Err(VaMError::UnsupportedEncoding(format!(
                "ZIP method {method} for {}::{}",
                path.display(),
                entry.name
            )));
        }
    };
    if output.len() != entry.uncompressed_size as usize {
        return Err(VaMError::InvalidSkinPreset {
            locator: format!("{}::{}", path.display(), entry.name),
            message: format!(
                "decoded {} bytes; directory declares {}",
                output.len(),
                entry.uncompressed_size
            ),
        });
    }
    Ok(output)
}

fn central_zip64_sizes(fixed: &[u8], extra: &[u8]) -> Result<(u64, u64, u64)> {
    let raw_compressed = le_u32(fixed, 20)?;
    let raw_uncompressed = le_u32(fixed, 24)?;
    let raw_offset = le_u32(fixed, 42)?;
    if raw_compressed != u32::MAX && raw_uncompressed != u32::MAX && raw_offset != u32::MAX {
        return Ok((
            raw_uncompressed as u64,
            raw_compressed as u64,
            raw_offset as u64,
        ));
    }

    let mut cursor = 0_usize;
    while cursor + 4 <= extra.len() {
        let identifier = le_u16(extra, cursor)?;
        let size = le_u16(extra, cursor + 2)? as usize;
        let start = cursor + 4;
        let end = start
            .checked_add(size)
            .ok_or_else(|| VaMError::InvalidSkinPreset {
                locator: "ZIP64".to_owned(),
                message: "extended field length overflow".to_owned(),
            })?;
        let field = extra
            .get(start..end)
            .ok_or_else(|| VaMError::InvalidSkinPreset {
                locator: "ZIP64".to_owned(),
                message: "extended field is truncated".to_owned(),
            })?;
        if identifier == 0x0001 {
            let mut field_cursor = 0_usize;
            let mut next_u64 = || -> Result<u64> {
                let value = le_u64(field, field_cursor)?;
                field_cursor += 8;
                Ok(value)
            };
            let uncompressed = if raw_uncompressed == u32::MAX {
                next_u64()?
            } else {
                raw_uncompressed as u64
            };
            let compressed = if raw_compressed == u32::MAX {
                next_u64()?
            } else {
                raw_compressed as u64
            };
            let offset = if raw_offset == u32::MAX {
                next_u64()?
            } else {
                raw_offset as u64
            };
            return Ok((uncompressed, compressed, offset));
        }
        cursor = end;
    }
    Err(VaMError::UnsupportedEncoding(
        "ZIP entry uses ZIP64 sentinels without a ZIP64 extra field".to_owned(),
    ))
}

fn le_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    let value: [u8; 2] = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| VaMError::InvalidSkinPreset {
            locator: "ZIP".to_owned(),
            message: "numeric field is truncated".to_owned(),
        })?
        .try_into()
        .expect("two bytes");
    Ok(u16::from_le_bytes(value))
}

fn le_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let value: [u8; 4] = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| VaMError::InvalidSkinPreset {
            locator: "ZIP".to_owned(),
            message: "numeric field is truncated".to_owned(),
        })?
        .try_into()
        .expect("four bytes");
    Ok(u32::from_le_bytes(value))
}

fn le_u64(bytes: &[u8], offset: usize) -> Result<u64> {
    let value: [u8; 8] = bytes
        .get(offset..offset + 8)
        .ok_or_else(|| VaMError::InvalidSkinPreset {
            locator: "ZIP64".to_owned(),
            message: "numeric field is truncated".to_owned(),
        })?
        .try_into()
        .expect("eight bytes");
    Ok(u64::from_le_bytes(value))
}

fn walk_files_if_exists(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut stack = vec![root.to_path_buf()];
    let mut result = Vec::new();
    while let Some(folder) = stack.pop() {
        let mut entries = fs::read_dir(&folder)
            .map_err(|error| io_error(&folder, error))?
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(|error| io_error(&folder, error))?;
        entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());
        for entry in entries.into_iter().rev() {
            let path = entry.path();
            let kind = entry.file_type().map_err(|error| io_error(&path, error))?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file() {
                result.push(path);
                if result.len() > MAX_WALK_FILES {
                    return Err(VaMError::InvalidSkinPreset {
                        locator: root.display().to_string(),
                        message: format!("file count exceeds {MAX_WALK_FILES}"),
                    });
                }
            }
        }
    }
    result.sort_by_key(|path| path.to_string_lossy().to_ascii_lowercase());
    Ok(result)
}

fn read_bounded_file(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    AssetLocator::File(path.to_path_buf()).read_bytes(maximum)
}

fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
}

fn is_skin_or_appearance_path(path: &Path) -> bool {
    is_skin_or_appearance_text(&path_to_forward_slashes(path))
}

fn is_skin_or_appearance_text(path: &str) -> bool {
    let normalized = format!("/{}/", path.replace('\\', "/").to_ascii_lowercase());
    normalized.contains("/custom/atom/person/skin/")
        || normalized.contains("/custom/atom/person/appearance/")
}

fn path_to_forward_slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn normalized_archive_path(path: &str) -> String {
    path.trim_start_matches(['/', '\\'])
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn normalized_id(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn write_stored_var(path: &Path, files: &[(&str, &[u8])]) {
        let mut output = Vec::new();
        let mut central = Vec::new();
        for (name, data) in files {
            let local_offset = output.len() as u32;
            output.extend_from_slice(b"PK\x03\x04");
            output.extend_from_slice(&20_u16.to_le_bytes());
            output.extend_from_slice(&0_u16.to_le_bytes());
            output.extend_from_slice(&0_u16.to_le_bytes());
            output.extend_from_slice(&0_u16.to_le_bytes());
            output.extend_from_slice(&0_u16.to_le_bytes());
            output.extend_from_slice(&0_u32.to_le_bytes());
            output.extend_from_slice(&(data.len() as u32).to_le_bytes());
            output.extend_from_slice(&(data.len() as u32).to_le_bytes());
            output.extend_from_slice(&(name.len() as u16).to_le_bytes());
            output.extend_from_slice(&0_u16.to_le_bytes());
            output.extend_from_slice(name.as_bytes());
            output.extend_from_slice(data);

            central.extend_from_slice(b"PK\x01\x02");
            central.extend_from_slice(&20_u16.to_le_bytes());
            central.extend_from_slice(&20_u16.to_le_bytes());
            central.extend_from_slice(&0_u16.to_le_bytes());
            central.extend_from_slice(&0_u16.to_le_bytes());
            central.extend_from_slice(&0_u16.to_le_bytes());
            central.extend_from_slice(&0_u16.to_le_bytes());
            central.extend_from_slice(&0_u32.to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&0_u16.to_le_bytes());
            central.extend_from_slice(&0_u16.to_le_bytes());
            central.extend_from_slice(&0_u16.to_le_bytes());
            central.extend_from_slice(&0_u16.to_le_bytes());
            central.extend_from_slice(&0_u32.to_le_bytes());
            central.extend_from_slice(&local_offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let central_offset = output.len() as u32;
        output.extend_from_slice(&central);
        output.extend_from_slice(b"PK\x05\x06");
        output.extend_from_slice(&0_u16.to_le_bytes());
        output.extend_from_slice(&0_u16.to_le_bytes());
        output.extend_from_slice(&(files.len() as u16).to_le_bytes());
        output.extend_from_slice(&(files.len() as u16).to_le_bytes());
        output.extend_from_slice(&(central.len() as u32).to_le_bytes());
        output.extend_from_slice(&central_offset.to_le_bytes());
        output.extend_from_slice(&0_u16.to_le_bytes());
        let mut file = File::create(path).unwrap();
        file.write_all(&output).unwrap();
    }

    fn write_cache_pair(directory: &Path, name: &str) -> PathBuf {
        fs::create_dir_all(directory).unwrap();
        let meta = directory.join(format!("{name}.vamcachemeta"));
        fs::write(&meta, b"meta").unwrap();
        fs::write(meta.with_extension("vamcache"), b"cache").unwrap();
        meta
    }

    #[test]
    fn parses_face_and_torso_diffuse_urls() {
        let value = br#"{
          "storables": [{
            "id": "textures",
            "faceDiffuseUrl": "Custom/Atom/Person/Textures/Test/faceD.png",
            "torsoDiffuseUrl": "Custom/Atom/Person/Textures/Test/torsoD.png",
            "faceNormalUrl": "missing.package.1:/Custom/Atom/Person/Textures/Test/faceN.png",
            "faceSpecularUrl": "missing.package.1:/Custom/Atom/Person/Textures/Test/faceS.png",
            "faceGlossUrl": "missing.package.1:/Custom/Atom/Person/Textures/Test/faceG.png"
          }]
        }"#;
        let temporary = std::env::temp_dir().join(format!("vkit-vam-skin-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temporary);
        fs::create_dir_all(temporary.join("VaM_Data").join("StreamingAssets")).unwrap();
        fs::write(
            temporary
                .join("VaM_Data")
                .join("StreamingAssets")
                .join("f_mb"),
            [],
        )
        .unwrap();
        let face = temporary
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Textures")
            .join("Test")
            .join("faceD.png");
        fs::create_dir_all(face.parent().unwrap()).unwrap();
        fs::write(&face, b"image").unwrap();
        let torso = face.with_file_name("torsoD.png");
        fs::write(&torso, b"torso").unwrap();
        let root = VaMRoot::open(&temporary).unwrap();
        let packages = ScanPackages {
            archives: Vec::new(),
            archive_by_uid: BTreeMap::new(),
            expanded: Vec::new(),
            expanded_by_uid: BTreeMap::new(),
            cache_textures: Vec::new(),
        };
        let preset = parse_vap(
            value,
            AssetLocator::File(temporary.join("skin.vap")),
            "Test",
            PresetContainer::Local,
            &root,
            &packages,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            preset.diffuse(SkinRegion::Face).cloned(),
            Some(AssetLocator::File(fs::canonicalize(face).unwrap()))
        );
        assert_eq!(
            preset.diffuse(SkinRegion::Torso).cloned(),
            Some(AssetLocator::File(fs::canonicalize(torso).unwrap()))
        );
        assert!(matches!(
            preset.surface(SkinRegion::Face).normal,
            Some(AssetLocator::UnresolvedVaMUrl(_))
        ));
        assert!(matches!(
            preset.surface(SkinRegion::Face).specular,
            Some(AssetLocator::UnresolvedVaMUrl(_))
        ));
        assert!(matches!(
            preset.surface(SkinRegion::Face).gloss,
            Some(AssetLocator::UnresolvedVaMUrl(_))
        ));
        fs::remove_dir_all(temporary).unwrap();
    }

    #[test]
    fn parses_auxiliary_material_urls_and_hsv_tints() {
        let value = br#"{
          "storables": [
            {"id":"textures","faceDiffuseUrl":"Custom/Atom/Person/Textures/Test/face.png"},
            {"id":"teeth","customTexture_MainTex":"missing:/teeth.png",
             "customTexture_BumpMap":"missing:/teethN.png",
             "customTexture_SpecTex":"missing:/teethS.png",
             "customTexture_GlossTex":"missing:/teethG.png",
             "Diffuse Color":{"h":"0","s":"0","v":"0.5"}},
            {"id":"MaleEyelashes","Eyelash":"Lashes 14",
             "customTexture_AlphaTex":"missing:/lashes.jpg",
             "Diffuse Color":{"h":"0","s":"1","v":"1"}}
          ]
        }"#;
        let temporary =
            std::env::temp_dir().join(format!("vkit-vam-aux-material-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temporary);
        fs::create_dir_all(temporary.join("VaM_Data").join("StreamingAssets")).unwrap();
        fs::write(
            temporary
                .join("VaM_Data")
                .join("StreamingAssets")
                .join("f_mb"),
            [],
        )
        .unwrap();
        let face = temporary
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Textures")
            .join("Test")
            .join("face.png");
        fs::create_dir_all(face.parent().unwrap()).unwrap();
        fs::write(face, b"face").unwrap();
        let root = VaMRoot::open(&temporary).unwrap();
        let packages = ScanPackages {
            archives: Vec::new(),
            archive_by_uid: BTreeMap::new(),
            expanded: Vec::new(),
            expanded_by_uid: BTreeMap::new(),
            cache_textures: Vec::new(),
        };
        let preset = parse_vap(
            value,
            AssetLocator::File(temporary.join("appearance.vap")),
            "Appearance",
            PresetContainer::Local,
            &root,
            &packages,
        )
        .unwrap()
        .unwrap();
        assert_eq!(preset.auxiliary.teeth.base_color, [128, 128, 128, 255]);
        assert_eq!(preset.auxiliary.eyelashes.base_color, [255, 0, 0, 255]);
        assert!(matches!(
            preset.auxiliary.teeth.diffuse,
            Some(AssetLocator::UnresolvedVaMUrl(_))
        ));
        assert!(matches!(
            preset.auxiliary.teeth.surface.normal,
            Some(AssetLocator::UnresolvedVaMUrl(_))
        ));
        assert!(matches!(
            preset.auxiliary.teeth.surface.specular,
            Some(AssetLocator::UnresolvedVaMUrl(_))
        ));
        assert!(matches!(
            preset.auxiliary.teeth.surface.gloss,
            Some(AssetLocator::UnresolvedVaMUrl(_))
        ));
        assert!(matches!(
            preset.auxiliary.eyelashes.diffuse,
            Some(AssetLocator::UnresolvedVaMUrl(_))
        ));
        assert_eq!(
            preset.auxiliary.teeth.diffuse_source,
            SkinDiffuseSource::CustomTexture
        );
        assert_eq!(
            preset.auxiliary.eyelashes.diffuse_source,
            SkinDiffuseSource::CustomTexture,
            "an explicit texture URL must take precedence over the built-in selector"
        );
        fs::remove_dir_all(temporary).unwrap();
    }

    #[test]
    fn built_in_auxiliary_selectors_choose_matching_cache_assets_and_figure_lashes() {
        let temporary =
            std::env::temp_dir().join(format!("vkit-vam-aux-selectors-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temporary);
        let streaming = temporary.join("VaM_Data").join("StreamingAssets");
        fs::create_dir_all(&streaming).unwrap();
        fs::write(streaming.join("f_mb"), []).unwrap();
        let face = temporary
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Textures")
            .join("Test")
            .join("face.png");
        fs::create_dir_all(face.parent().unwrap()).unwrap();
        fs::write(&face, b"face").unwrap();
        let cache = temporary.join("Cache").join("Textures");
        let female_lashes = write_cache_pair(&cache, "Female_Lashes14");
        let male_lashes = write_cache_pair(&cache, "Male_Lashes1");

        let iris = write_cache_pair(
            &cache,
            "Preset_DualColor11_jpg_59215_132878108000000000_512_512_C",
        );
        let sclera = write_cache_pair(&cache, "Sclera2");
        let root = VaMRoot::open(&temporary).unwrap();
        let packages = ScanPackages {
            archives: Vec::new(),
            archive_by_uid: BTreeMap::new(),
            expanded: Vec::new(),
            expanded_by_uid: BTreeMap::new(),
            cache_textures: vec![
                female_lashes.clone(),
                male_lashes,
                iris.clone(),
                sclera.clone(),
            ],
        };
        let value = br#"{"storables":[
            {"id":"geometry","character":"Female 1"},
            {"id":"FemaleEyelashes","Eyelash":"Lashes 14","customTexture_AlphaTex":"","Diffuse Color":{"h":"0","s":"1","v":"1"}},
            {"id":"MaleEyelashes","Eyelash":"Lashes 1","customTexture_AlphaTex":"","Diffuse Color":{"h":"0.5","s":"1","v":"1"}},
            {"id":"irises","Irises":"Color 11","customTexture_MainTex":""},
            {"id":"sclera","Sclera":"Sclera 2","customTexture_MainTex":""},
            {"id":"textures","faceDiffuseUrl":"Custom/Atom/Person/Textures/Test/face.png"}
        ]}"#;
        let preset = parse_vap(
            value,
            AssetLocator::File(temporary.join("selector.vap")),
            "Selector",
            PresetContainer::Local,
            &root,
            &packages,
        )
        .unwrap()
        .unwrap();

        assert_eq!(preset.sex, SkinSex::Female);
        assert_eq!(preset.auxiliary.eyelashes.base_color, [255, 0, 0, 255]);
        assert_eq!(
            preset.auxiliary.eyelashes.diffuse,
            Some(AssetLocator::File(female_lashes))
        );
        assert_eq!(
            preset.auxiliary.eyelashes.diffuse_source,
            SkinDiffuseSource::BuiltInSelectorResolved("Lashes 14".to_owned())
        );
        assert_eq!(
            preset.auxiliary.iris.diffuse,
            Some(AssetLocator::File(iris))
        );
        assert_eq!(
            preset.auxiliary.iris.diffuse_source,
            SkinDiffuseSource::BuiltInSelectorResolved("Color 11".to_owned())
        );
        assert_eq!(
            preset.auxiliary.sclera.diffuse,
            Some(AssetLocator::File(sclera))
        );
        assert_eq!(
            preset.auxiliary.sclera.diffuse_source,
            SkinDiffuseSource::BuiltInSelectorResolved("Sclera 2".to_owned())
        );
        assert!(preset.auxiliary.unavailable_builtin_selectors().is_empty());
        fs::remove_dir_all(temporary).unwrap();
    }

    #[test]
    #[ignore = "diagnostic: requires a local VaM installation with populated texture cache"]
    fn real_vam_selector_cache_reports_resolved_and_asset_bundle_only_variants() {
        let root_path = std::env::var_os("VKIT_VAM_ROOT")
            .map(PathBuf::from)
            .expect("set VKIT_VAM_ROOT to a VaM install");
        let root = VaMRoot::open(&root_path).expect("set VKIT_VAM_ROOT to a VaM install");
        let temporary = std::env::temp_dir().join(format!(
            "vkit-vam-real-selector-face-{}",
            std::process::id()
        ));
        fs::write(&temporary, b"face").unwrap();
        let value = serde_json::to_vec(&serde_json::json!({
            "storables": [
                {"id": "geometry", "character": "Female 1"},
                {"id": "FemaleEyelashes", "Eyelash": "Lashes 14", "customTexture_AlphaTex": ""},
                {"id": "irises", "Irises": "Color 11", "customTexture_MainTex": ""},
                {"id": "sclera", "Sclera": "Sclera 2", "customTexture_MainTex": ""},
                {"id": "textures", "faceDiffuseUrl": temporary.to_string_lossy()}
            ]
        }))
        .unwrap();
        let packages = ScanPackages {
            archives: Vec::new(),
            archive_by_uid: BTreeMap::new(),
            expanded: Vec::new(),
            expanded_by_uid: BTreeMap::new(),
            cache_textures: cache_texture_entries(&root).unwrap(),
        };
        let preset = parse_vap(
            &value,
            AssetLocator::File(root_path.join("selector-diagnostic.vap")),
            "Selector diagnostic",
            PresetContainer::Local,
            &root,
            &packages,
        )
        .unwrap()
        .unwrap();

        assert_eq!(
            preset.auxiliary.iris.diffuse_source,
            SkinDiffuseSource::BuiltInSelectorResolved("Color 11".to_owned())
        );
        assert_eq!(
            preset.auxiliary.sclera.diffuse_source,
            SkinDiffuseSource::BuiltInSelectorUnavailable("Sclera 2".to_owned())
        );
        assert_eq!(
            preset.auxiliary.eyelashes.diffuse_source,
            SkinDiffuseSource::BuiltInSelectorUnavailable("Lashes 14".to_owned())
        );
        assert_eq!(
            preset.auxiliary.unavailable_builtin_selectors(),
            vec![("sclera", "Sclera 2"), ("eyelashes", "Lashes 14")]
        );
        fs::remove_file(temporary).unwrap();
    }

    #[test]
    fn sex_specific_cache_fallback_never_crosses_an_explicit_figure_family() {
        let entries = vec![
            PathBuf::from("Female_V5BreeLashes1.vamcachemeta"),
            PathBuf::from("Male_V5BreeLashes1.vamcachemeta"),
            PathBuf::from("Neutral_V5BreeLashes1.vamcachemeta"),
        ];
        assert_eq!(
            find_cache_profile_for_sex(&entries, EYELASH_CACHE_PROFILES, SkinSex::Female),
            Some(entries[0].clone())
        );
        assert_eq!(
            find_cache_profile_for_sex(&entries, EYELASH_CACHE_PROFILES, SkinSex::Male),
            Some(entries[1].clone())
        );
        assert_eq!(
            find_cache_profile_for_sex(&entries[..1], EYELASH_CACHE_PROFILES, SkinSex::Male),
            None
        );
        assert_eq!(
            find_cache_profile_for_sex(&entries[2..], EYELASH_CACHE_PROFILES, SkinSex::Female),
            Some(entries[2].clone())
        );
    }

    fn bundle_texture(name: &str) -> AssetLocator {
        AssetLocator::BuiltinTexture(std::sync::Arc::new(
            super::super::unity_textures::BuiltinTextureRef {
                bundle_path: PathBuf::from("f_1_mat"),
                cab_node: "CAB-test".to_owned(),
                path_id: 1,
                texture_name: name.to_owned(),
                normal_map: false,
                cache_directory: None,
            },
        ))
    }

    /// What someone who has just installed VaM and never run it actually has.
    ///
    /// Both defects this guards against looked like quality problems and were
    /// really availability ones: the UV mapping came from a loose OBJ and the
    /// eye, mouth and lash textures from `Cache/Textures`, and neither exists
    /// until VaM has been used. Point `VKIT_FRESH_ROOT` at a directory holding
    /// nothing but `VaM_Data/StreamingAssets` — hard links to the real ones
    /// are enough, and cost nothing:
    ///
    /// ```text
    /// Get-ChildItem -File <VaM>\VaM_Data\StreamingAssets | ForEach-Object {
    ///     New-Item -ItemType HardLink -Path (Join-Path $dst $_.Name) -Target $_.FullName
    /// }
    /// ```
    #[test]
    #[ignore = "reads a stripped copy of a VaM installation"]
    fn a_first_run_installation_still_dresses_the_eyes_and_mouth() {
        let Ok(path) = std::env::var("VKIT_FRESH_ROOT") else {
            return;
        };
        let root = VaMRoot::open(std::path::Path::new(&path)).unwrap();
        let report = scan_skin_library_with_report(&root).unwrap();
        println!("presets: {}", report.presets.len());
        println!("warnings: {:?}", report.diagnostics.warnings);

        for (figure, sex, geometry_sex) in [
            (
                "f_1",
                SkinSex::Female,
                super::super::geometry::GeometrySex::Female,
            ),
            (
                "m_1",
                SkinSex::Male,
                super::super::geometry::GeometrySex::Male,
            ),
        ] {
            let mut base = report
                .presets
                .iter()
                .find(|preset| {
                    preset.stable_id == super::super::unity_textures::builtin_stable_id(figure)
                })
                .cloned()
                .unwrap_or_else(|| panic!("{figure} is not in the skin library"));
            for material in base.auxiliary.materials_mut() {
                assert!(material.diffuse.is_some(), "{figure} has a bare region");
            }

            let discovered = super::super::base_discovery::discover_geometry_base(
                super::super::base_discovery::GeometryBaseRequest {
                    root: Some(&root),
                    sex: geometry_sex,
                    licensed_anchor: None,
                    explicit_candidates: &[],
                    cache_dir: None,
                },
            )
            .unwrap();
            let mapping = super::super::uv::load_g2_uv_mapping_for_sex(
                &root,
                discovered.provider.daz_anchor(),
                sex,
            )
            .unwrap();
            println!(
                "{figure}: base {:?} from {} | uv from {} ({} faces, {} uncovered)",
                discovered.tier,
                discovered.base_path.display(),
                mapping.source_path.display(),
                mapping.faces.len(),
                mapping.uncovered_triangles
            );
            // The whole point: the mapping came out of the figure bundle, not
            // out of a loose file this installation may not have.
            assert_eq!(
                mapping.source_path,
                root.neutral_base_bundle_path(geometry_sex),
                "{figure} fell back to a loose mesh"
            );
            assert_eq!(mapping.uncovered_triangles, 0);

            // The morphs a face is actually shaped with, from the same kind of
            // place: a bundle that is part of the installation.
            let bank =
                super::super::unity_morph_bank::BuiltinMorphSession::open_for_sex(&root, sex)
                    .unwrap();
            println!(
                "{figure}: {} built-in morphs from {}",
                bank.morph_count(),
                bank.bundle_path().display()
            );
            // The floor is low on purpose: the point is that the bank decodes
            // to real morphs out of the installation rather than to an empty
            // list, not that either figure carries some particular number.
            assert!(
                bank.morph_count() > 100,
                "{figure} decoded {} morphs",
                bank.morph_count()
            );
            assert!(
                bank.bundle_path()
                    .starts_with(root.path().join("VaM_Data").join("StreamingAssets")),
                "{figure} read its morphs from outside the installation: {}",
                bank.bundle_path().display()
            );
        }
    }

    /// The report behind routing these defaults through the bundle: on a real
    /// installation every eye, mouth and lash texture is present before VaM has
    /// ever rendered, which is exactly what `Cache/Textures` cannot promise.
    #[test]
    #[ignore = "reads the reader's own VaM installation"]
    fn every_stand_in_region_resolves_from_the_installation_alone() {
        let Ok(path) = std::env::var("VKIT_VAM_ROOT") else {
            return;
        };
        let root = VaMRoot::open(std::path::Path::new(&path)).unwrap();
        let (builtin, _) = super::super::unity_textures::scan_builtin_skins(&root, None).unwrap();

        for sex in [SkinSex::Female, SkinSex::Male] {
            let mut defaults = base_figure_auxiliary(&builtin, sex);
            let named = [
                "sclera",
                "iris",
                "lacrimal",
                "inner mouth",
                "teeth",
                "gums",
                "tongue",
                "eyelashes",
            ];
            for (region, material) in named.iter().zip(defaults.materials_mut()) {
                assert!(
                    matches!(material.diffuse, Some(AssetLocator::BuiltinTexture(_))),
                    "{sex:?} {region} has no bundle texture"
                );
            }
            assert!(
                iris_is_the_default_color(defaults.iris.diffuse.as_ref()),
                "{sex:?} iris must be able to stand in"
            );
            // Surface maps ride along wherever the figure paints them.
            assert!(defaults.teeth.surface.specular.is_some());
            assert!(defaults.inner_mouth.surface.specular.is_some());
            assert!(defaults.sclera.surface.normal.is_some());
        }
    }

    #[test]
    fn the_base_figure_supplies_the_stand_ins_without_any_texture_cache() {
        let mut base = SkinPreset {
            stable_id: super::super::unity_textures::builtin_stable_id("f_1"),
            label: "base".to_owned(),
            source: AssetLocator::File(PathBuf::from("f_1_mat")),
            sex: SkinSex::Female,
            textures: SkinTextureSet::default(),
            auxiliary: SkinAuxiliary::default(),
        };
        for (index, material) in base.auxiliary.materials_mut().into_iter().enumerate() {
            material.diffuse = Some(bundle_texture(&format!("region-{index}")));
            material.diffuse_source = SkinDiffuseSource::CustomTexture;
        }
        let others = vec![base];

        let mut defaults = base_figure_auxiliary(&others, SkinSex::Female);
        for material in defaults.materials_mut() {
            assert!(
                material.diffuse.is_some(),
                "every stand-in region comes from the bundle"
            );
            assert_eq!(material.diffuse_source, SkinDiffuseSource::FigureDefault);
        }
        // A figure this installation does not carry leaves the stand-ins empty
        // rather than borrowing another figure's.
        assert!(
            base_figure_auxiliary(&others, SkinSex::Male)
                .materials_mut()
                .into_iter()
                .all(|material| material.diffuse.is_none())
        );
    }

    #[test]
    fn a_look_that_names_one_eye_texture_wears_it_in_all_three_eye_regions() {
        let own = AssetLocator::File(PathBuf::from("this-looks-own-eye.png"));
        let mut stand_in = SkinAuxiliary::default();
        stand_in.sclera.diffuse = Some(bundle_texture("base-eye"));
        stand_in.iris.diffuse = Some(bundle_texture("base-eye"));
        stand_in.lacrimal.diffuse = Some(bundle_texture("base-eye"));
        stand_in.teeth.diffuse = Some(bundle_texture("base-teeth"));

        let mut look = SkinAuxiliary::default();
        look.sclera.diffuse = Some(own.clone());
        look.sclera.diffuse_source = SkinDiffuseSource::CustomTexture;

        look.spread_own_eye_atlas();
        look.fill_missing_from(&stand_in, true);

        assert_eq!(look.sclera.diffuse.as_ref(), Some(&own));
        assert_eq!(
            look.iris.diffuse.as_ref(),
            Some(&own),
            "the iris takes the look's own eye, not the base figure's"
        );
        assert_eq!(look.lacrimal.diffuse.as_ref(), Some(&own));
        // Regions it named nothing for still come from the stand-in.
        assert_eq!(look.teeth.diffuse, stand_in.teeth.diffuse);
    }

    #[test]
    fn a_look_that_names_no_eye_takes_the_base_figures() {
        let mut stand_in = SkinAuxiliary::default();
        stand_in.sclera.diffuse = Some(bundle_texture("base-eye"));
        stand_in.sclera.diffuse_source = SkinDiffuseSource::FigureDefault;

        let mut look = SkinAuxiliary::default();
        look.spread_own_eye_atlas();
        look.fill_missing_from(&stand_in, true);

        assert_eq!(look.sclera.diffuse, stand_in.sclera.diffuse);
    }

    #[test]
    fn only_the_figures_own_iris_may_stand_in_for_a_missing_color() {
        assert!(iris_is_the_default_color(Some(&bundle_texture("iris"))));
        assert!(iris_is_the_default_color(Some(&AssetLocator::File(
            PathBuf::from("Preset_DualColor01_jpg_38947_132878108620000000_512_512_C.vamcachemeta")
        ))));
        assert!(!iris_is_the_default_color(Some(&AssetLocator::File(
            PathBuf::from("Preset_DualColor07_jpg_38947_132878108620000000_512_512_C.vamcachemeta")
        ))));
        assert!(!iris_is_the_default_color(Some(&AssetLocator::File(
            PathBuf::from("V5BreeEyes8M_jpg.vamcachemeta")
        ))));
        assert!(!iris_is_the_default_color(None));
    }

    #[test]
    fn unavailable_selector_uses_neutral_preview_without_losing_its_identity() {
        let mut outer = SkinAuxiliary::default();
        outer.sclera.diffuse_source =
            SkinDiffuseSource::BuiltInSelectorUnavailable("Sclera 2".to_owned());
        let mut inherited = SkinAuxiliary::default();
        inherited.sclera.diffuse = Some(AssetLocator::File(PathBuf::from("generic-eye")));
        inherited.sclera.diffuse_source = SkinDiffuseSource::FigureDefault;

        outer.fill_missing_from(&inherited, true);

        assert_eq!(
            outer.sclera.diffuse,
            Some(AssetLocator::File(PathBuf::from("generic-eye")))
        );
        assert_eq!(
            outer.sclera.diffuse_source,
            SkinDiffuseSource::BuiltInSelectorUnavailable("Sclera 2".to_owned())
        );
    }

    #[test]
    fn unmatched_builtin_selectors_use_neutral_preview_and_keep_diagnostics() {
        let temporary = std::env::temp_dir().join(format!(
            "vkit-vam-selector-generic-fallback-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temporary);
        let streaming = temporary.join("VaM_Data").join("StreamingAssets");
        let appearance = temporary
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Appearance");
        let textures = temporary
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Textures")
            .join("Test");
        fs::create_dir_all(&streaming).unwrap();
        fs::create_dir_all(&appearance).unwrap();
        fs::create_dir_all(&textures).unwrap();
        fs::write(streaming.join("f_mb"), []).unwrap();
        fs::write(textures.join("face.png"), b"face").unwrap();
        let cache = temporary.join("Cache").join("Textures");
        let generic_eye = write_cache_pair(&cache, "V5BreeEyes8M");
        let generic_lashes = write_cache_pair(&cache, "V5BreeLashes1");
        fs::write(
            appearance.join("Selectors.vap"),
            br#"{"storables":[
                {"id":"geometry","character":"Female 1"},
                {"id":"FemaleEyelashes","Eyelash":"Lashes 99","customTexture_AlphaTex":""},
                {"id":"irises","Irises":"Color 99","customTexture_MainTex":""},
                {"id":"sclera","Sclera":"Sclera 99","customTexture_MainTex":""},
                {"id":"textures","faceDiffuseUrl":"Custom/Atom/Person/Textures/Test/face.png"}
            ]}"#,
        )
        .unwrap();

        let root = VaMRoot::open(&temporary).unwrap();
        let report = scan_skin_library_with_report(&root).unwrap();
        // Silent by design: showing the eye underneath is the decision, so a
        // look that named a colour this installation lacks is not a warning.
        // The selector is still recorded on the preset, which is what a future
        // eye-colour feature would read.
        assert!(
            !report
                .diagnostics
                .warnings
                .iter()
                .any(|warning| warning.contains("selector")),
            "{:?}",
            report.diagnostics.warnings
        );
        let preset = report
            .presets
            .iter()
            .find(|preset| preset.label == "Selectors")
            .unwrap();
        assert_eq!(
            preset.auxiliary.eyelashes.diffuse,
            Some(AssetLocator::File(generic_lashes.canonicalize().unwrap()))
        );
        assert_eq!(
            preset.auxiliary.iris.diffuse,
            Some(AssetLocator::File(generic_eye.canonicalize().unwrap())),
            "the rendered iris must use the complete shared eye atlas"
        );
        assert_eq!(
            preset.auxiliary.sclera.diffuse,
            Some(AssetLocator::File(generic_eye.canonicalize().unwrap()))
        );
        assert_eq!(
            preset.auxiliary.eyelashes.diffuse_source,
            SkinDiffuseSource::BuiltInSelectorUnavailable("Lashes 99".to_owned())
        );
        assert_eq!(
            preset.auxiliary.iris.diffuse_source,
            SkinDiffuseSource::BuiltInSelectorUnavailable("Color 99".to_owned())
        );
        assert_eq!(
            preset.auxiliary.sclera.diffuse_source,
            SkinDiffuseSource::BuiltInSelectorUnavailable("Sclera 99".to_owned())
        );
        assert_eq!(
            preset.auxiliary.unavailable_builtin_selectors(),
            vec![
                ("sclera", "Sclera 99"),
                ("iris", "Color 99"),
                ("eyelashes", "Lashes 99"),
            ]
        );
        assert!(generic_eye.is_file() && generic_lashes.is_file());
        fs::remove_dir_all(temporary).unwrap();
    }

    #[test]
    fn selector_numeric_suffix_never_matches_a_longer_builtin_number() {
        let color_10 = PathBuf::from("Preset_DualColor10_jpg.vamcachemeta");
        let color_1 = PathBuf::from("Preset_DualColor1_jpg.vamcachemeta");
        let sclera_10 = PathBuf::from("Sclera10_png.vamcachemeta");
        let lashes_11 = PathBuf::from("Female_Lashes11_png.vamcachemeta");
        let entries = vec![color_10.clone(), sclera_10, lashes_11];

        assert_eq!(
            find_selector_cache(
                &entries,
                AuxiliarySelectorRegion::Iris,
                "Color 1",
                SkinSex::Female,
            ),
            None
        );
        assert_eq!(
            find_selector_cache(
                &entries,
                AuxiliarySelectorRegion::Sclera,
                "Sclera 1",
                SkinSex::Female,
            ),
            None
        );
        assert_eq!(
            find_selector_cache(
                &entries,
                AuxiliarySelectorRegion::Eyelashes,
                "Lashes 1",
                SkinSex::Female,
            ),
            None
        );

        let exact = [color_10, color_1.clone()];
        assert_eq!(
            find_selector_cache(
                &exact,
                AuxiliarySelectorRegion::Iris,
                "Color 1",
                SkinSex::Female,
            ),
            Some(color_1)
        );
    }

    #[test]
    fn default_full_eye_atlas_covers_iris_sclera_and_lacrimal() {
        let generic_eye = PathBuf::from("V5BreeEyes8M_jpg.vamcachemeta");
        let iris = PathBuf::from("Preset_DualColor01_jpg.vamcachemeta");
        let auxiliary =
            default_auxiliary_from_cache(&[generic_eye.clone(), iris.clone()], SkinSex::Unknown);
        assert_eq!(
            auxiliary.sclera.diffuse,
            Some(AssetLocator::File(generic_eye.clone()))
        );
        assert_eq!(
            auxiliary.lacrimal.diffuse,
            Some(AssetLocator::File(generic_eye.clone()))
        );
        assert_eq!(
            auxiliary.iris.diffuse,
            Some(AssetLocator::File(generic_eye.clone()))
        );

        let only_generic_eye =
            default_auxiliary_from_cache(std::slice::from_ref(&generic_eye), SkinSex::Unknown);
        assert_eq!(
            only_generic_eye.iris.diffuse,
            Some(AssetLocator::File(generic_eye))
        );
    }

    #[test]
    fn default_iris_accepts_only_the_genuine_vam_color_one_cache_name() {
        let thumbnail = PathBuf::from("Preset_DualColor01_thumbnail_png.vamcachemeta");
        let reflection = PathBuf::from("Preset_DualColor01_Reflection_jpg.vamcachemeta");
        let addon = PathBuf::from("Creator_Preset_DualColor01_jpg.vamcachemeta");
        let builtin =
            PathBuf::from("Preset_DualColor01_jpg_38947_132878108620000000_512_512_C.vamcachemeta");

        let auxiliary = default_auxiliary_from_cache(
            &[thumbnail, reflection, addon, builtin.clone()],
            SkinSex::Unknown,
        );

        assert_eq!(auxiliary.iris.diffuse, Some(AssetLocator::File(builtin)));
        assert_eq!(
            builtin_iris_cache_color_number(Path::new(
                "Preset_DualColor01_thumbnail_png.vamcachemeta"
            )),
            None
        );
        assert_eq!(
            builtin_iris_cache_color_number(Path::new(
                "Preset_DualColor01_Reflection_jpg.vamcachemeta"
            )),
            None
        );
    }

    #[test]
    fn colliding_dualcolor_cache_basenames_use_procedural_iris_fallback() {
        let built_in_candidate = PathBuf::from(
            "Cache/Textures/Preset_DualColor01_jpg_38947_132878108620000000_512_512_C.vamcachemeta",
        );
        let hair_colors_candidate = PathBuf::from(
            "Cache/Textures/Preset_DualColor01_jpg_38947_132878072620000000_512_512_C.vamcachemeta",
        );
        let entries = [built_in_candidate, hair_colors_candidate];

        assert_eq!(
            find_builtin_iris_cache(&entries, DEFAULT_BUILTIN_IRIS_COLOR, SkinSex::Unknown),
            None
        );
        let auxiliary = default_auxiliary_from_cache(&entries, SkinSex::Unknown);
        assert!(auxiliary.iris.diffuse.is_none());
        assert_eq!(auxiliary.iris.diffuse_source, SkinDiffuseSource::None);
    }

    #[test]
    fn iris_selector_matches_zero_padded_builtin_and_rejects_unrelated_iris_files() {
        let unrelated = PathBuf::from("Iris_Color01_Reflection_jpg.vamcachemeta");
        let color_10 = PathBuf::from("Preset_DualColor10_jpg.vamcachemeta");
        let color_1 = PathBuf::from("Preset_DualColor01_jpg.vamcachemeta");
        let entries = vec![unrelated.clone(), color_10.clone(), color_1.clone()];
        assert_eq!(
            find_selector_cache(
                &entries,
                AuxiliarySelectorRegion::Iris,
                "Color 1",
                SkinSex::Unknown,
            ),
            Some(color_1)
        );
        assert_eq!(
            find_selector_cache(
                &entries,
                AuxiliarySelectorRegion::Iris,
                "Color 10",
                SkinSex::Unknown,
            ),
            Some(color_10)
        );
        assert_eq!(
            find_selector_cache(
                &[unrelated],
                AuxiliarySelectorRegion::Iris,
                "Color 1",
                SkinSex::Unknown,
            ),
            None
        );
    }

    #[test]
    fn unresolved_iris_selector_never_inherits_a_generic_eye_image() {
        let mut outer = SkinAuxiliary::default();
        outer.iris.diffuse_source =
            SkinDiffuseSource::BuiltInSelectorUnavailable("Color 99".to_owned());
        let mut inherited = SkinAuxiliary::default();
        inherited.iris.diffuse = Some(AssetLocator::File(PathBuf::from("generic-eye")));
        inherited.iris.diffuse_source = SkinDiffuseSource::FigureDefault;

        outer.fill_missing_from(
            &inherited,
            iris_is_the_default_color(inherited.iris.diffuse.as_ref()),
        );

        assert!(outer.iris.diffuse.is_none());
        assert_eq!(
            outer.iris.diffuse_source,
            SkinDiffuseSource::BuiltInSelectorUnavailable("Color 99".to_owned())
        );
    }

    #[test]
    fn unavailable_iris_selector_falls_back_to_the_genuine_color_one_cache() {
        let color_one =
            PathBuf::from("Preset_DualColor01_jpg_38947_132878108620000000_512_512_C.vamcachemeta");
        let mut outer = SkinAuxiliary::default();
        outer.iris.diffuse_source =
            SkinDiffuseSource::BuiltInSelectorUnavailable("Color 99".to_owned());
        let fallback =
            default_auxiliary_from_cache(std::slice::from_ref(&color_one), SkinSex::Unknown);

        outer.fill_missing_from(
            &fallback,
            iris_is_the_default_color(fallback.iris.diffuse.as_ref()),
        );

        assert_eq!(outer.iris.diffuse, Some(AssetLocator::File(color_one)));
        assert_eq!(
            outer.iris.diffuse_source,
            SkinDiffuseSource::BuiltInSelectorUnavailable("Color 99".to_owned())
        );
    }

    #[test]
    fn missing_custom_iris_uses_a_valid_builtin_selector_before_defaulting() {
        let color_eleven =
            PathBuf::from("Preset_DualColor11_jpg_59215_132878108000000000_512_512_C.vamcachemeta");
        let mut parsed = ParsedAuxiliaryMaterials::default();
        parsed.auxiliary.iris.diffuse = Some(AssetLocator::UnresolvedVaMUrl(
            "missing.package:/thumbnail.png".to_owned(),
        ));
        parsed.auxiliary.iris.diffuse_source = SkinDiffuseSource::CustomTexture;
        parsed.iris_selector = Some("Color 11".to_owned());

        let auxiliary = parsed.finish(SkinSex::Unknown, std::slice::from_ref(&color_eleven));

        assert_eq!(
            auxiliary.iris.diffuse,
            Some(AssetLocator::File(color_eleven))
        );
        assert_eq!(
            auxiliary.iris.diffuse_source,
            SkinDiffuseSource::BuiltInSelectorResolved("Color 11".to_owned())
        );
    }

    #[test]
    fn public_skin_types_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AssetLocator>();
        assert_send_sync::<SkinPreset>();
    }

    #[test]
    fn malformed_vap_is_reported_without_aborting_the_library() {
        let temporary =
            std::env::temp_dir().join(format!("vkit-vam-resilient-skin-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temporary);
        let streaming = temporary.join("VaM_Data").join("StreamingAssets");
        let appearance = temporary
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Appearance");
        let textures = temporary
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Textures")
            .join("Test");
        fs::create_dir_all(&streaming).unwrap();
        fs::create_dir_all(&appearance).unwrap();
        fs::create_dir_all(&textures).unwrap();
        fs::write(streaming.join("f_mb"), []).unwrap();
        fs::write(appearance.join("bad.vap"), b"{not json").unwrap();
        fs::write(textures.join("faceD.png"), b"image").unwrap();
        fs::write(
            appearance.join("good.vap"),
            br#"{"storables":[{"id":"textures","faceDiffuseUrl":"Custom/Atom/Person/Textures/Test/faceD.png"}]}"#,
        )
        .unwrap();
        let root = VaMRoot::open(&temporary).unwrap();
        let report = scan_skin_library_with_report(&root).unwrap();
        assert_eq!(report.diagnostics.skipped_presets, 1);
        assert!(report.presets.iter().any(|preset| preset.label == "good"));
        fs::remove_dir_all(temporary).unwrap();
    }

    #[test]
    fn texture_folders_and_non_person_vaps_are_not_catalog_presets() {
        let temporary = std::env::temp_dir().join(format!(
            "vkit-vam-strict-skin-catalog-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temporary);
        let streaming = temporary.join("VaM_Data").join("StreamingAssets");
        let packages = temporary.join("AddonPackages");
        let local_textures = temporary
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Textures")
            .join("LooksLikeSkin");
        fs::create_dir_all(&streaming).unwrap();
        fs::create_dir_all(&packages).unwrap();
        fs::create_dir_all(&local_textures).unwrap();
        fs::write(streaming.join("f_mb"), []).unwrap();
        fs::write(local_textures.join("faceD.png"), b"image").unwrap();
        let clothing = br#"{"storables":[{"id":"textures","faceDiffuseUrl":"self:/Custom/Atom/Person/Textures/Clothing/faceD.png"}]}"#;
        write_stored_var(
            &packages.join("Creator.Clothing.1.var"),
            &[
                (
                    "Custom/Atom/Person/Clothing/Preset_NotASkin.vap",
                    clothing.as_slice(),
                ),
                ("Custom/Atom/Person/Textures/Clothing/faceD.png", b"image"),
            ],
        );

        let root = VaMRoot::open(&temporary).unwrap();
        let report = scan_skin_library_with_report(&root).unwrap();
        assert!(report.presets.is_empty());
        fs::remove_dir_all(temporary).unwrap();
    }

    #[test]
    fn nested_cross_var_skin_references_resolve_with_sex_and_cycle_guards() {
        let temporary =
            std::env::temp_dir().join(format!("vkit-vam-nested-skin-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temporary);
        let streaming = temporary.join("VaM_Data").join("StreamingAssets");
        let packages = temporary.join("AddonPackages");
        fs::create_dir_all(&streaming).unwrap();
        fs::create_dir_all(&packages).unwrap();
        fs::write(streaming.join("f_mb"), []).unwrap();

        let base = br#"{"storables":[{"id":"geometry","character":"Female Custom"},{"id":"textures","faceDiffuseUrl":"self:/Custom/Atom/Person/Textures/FemaleBase/Test/faceD.png","faceNormalUrl":"self:/Custom/Atom/Person/Textures/FemaleBase/Test/faceN.png"}]}"#;
        write_stored_var(
            &packages.join("Creator.Base.1.var"),
            &[
                ("Custom/Atom/Person/Skin/Base.vap", base.as_slice()),
                (
                    "Custom/Atom/Person/Textures/FemaleBase/Test/faceD.png",
                    b"image",
                ),
                (
                    "Custom/Atom/Person/Textures/FemaleBase/Test/faceN.png",
                    b"normal",
                ),
            ],
        );
        let wrapper = br#"{"storables":[{"id":"textures","faceDiffuseUrl":"self:/Custom/Atom/Person/Textures/Wrapper/faceD.png","torsoDiffuseUrl":"self:/Custom/Atom/Person/Textures/Wrapper/torsoD.png"},{"id":"skinPreset","preset":"Creator.Base.latest:/Custom/Atom/Person/Skin/Base.vap"}]}"#;
        let cycle = br#"{"storables":[{"id":"skinPreset","preset":"self:/Custom/Atom/Person/Appearance/Cycle.vap"}]}"#;
        let broken_nested = br#"{"storables":[{"id":"textures","faceDiffuseUrl":"self:/Custom/Atom/Person/Textures/Wrapper/faceD.png"},{"id":"skinPreset","preset":"Missing.Package.latest:/Custom/Atom/Person/Skin/Base.vap"}]}"#;
        write_stored_var(
            &packages.join("Creator.Wrapper.1.var"),
            &[
                (
                    "Custom/Atom/Person/Appearance/Wrapper.vap",
                    wrapper.as_slice(),
                ),
                ("Custom/Atom/Person/Appearance/Cycle.vap", cycle.as_slice()),
                (
                    "Custom/Atom/Person/Appearance/BrokenNested.vap",
                    broken_nested.as_slice(),
                ),
                ("Custom/Atom/Person/Textures/Wrapper/faceD.png", b"face"),
                ("Custom/Atom/Person/Textures/Wrapper/torsoD.png", b"torso"),
            ],
        );

        let root = VaMRoot::open(&temporary).unwrap();
        let report = scan_skin_library_with_report(&root).unwrap();
        let wrapper = report
            .presets
            .iter()
            .find(|preset| preset.label == "Wrapper")
            .unwrap();
        assert_eq!(wrapper.sex, SkinSex::Female);
        assert_eq!(
            wrapper
                .diffuse(SkinRegion::Face)
                .as_ref()
                .unwrap()
                .read_bytes(16)
                .unwrap(),
            b"face"
        );
        assert_eq!(
            wrapper
                .surface(SkinRegion::Face)
                .normal
                .as_ref()
                .unwrap()
                .read_bytes(16)
                .unwrap(),
            b"normal"
        );
        assert!(report.presets.iter().all(|preset| preset.label != "Cycle"));
        assert!(
            report
                .presets
                .iter()
                .all(|preset| preset.label != "BrokenNested")
        );
        assert_eq!(report.diagnostics.skipped_presets, 2);
        fs::remove_dir_all(temporary).unwrap();
    }

    #[test]
    fn shared_nested_base_is_not_mistaken_for_a_cycle() {
        let temporary = std::env::temp_dir().join(format!(
            "vkit-vam-shared-nested-base-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temporary);
        let streaming = temporary.join("VaM_Data").join("StreamingAssets");
        let packages = temporary.join("AddonPackages");
        fs::create_dir_all(&streaming).unwrap();
        fs::create_dir_all(&packages).unwrap();
        fs::write(streaming.join("f_mb"), []).unwrap();

        let common = br#"{"storables":[{"id":"geometry","character":"Female Custom"},{"id":"textures","faceDiffuseUrl":"self:/Custom/Atom/Person/Textures/Shared/faceD.png"}]}"#;
        let left = br#"{"storables":[{"id":"skinPreset","preset":"self:/Custom/Atom/Person/Skin/Common.vap"}]}"#;
        let right = br#"{"storables":[{"id":"skinPreset","preset":"self:/Custom/Atom/Person/Skin/Common.vap"}]}"#;
        let root_preset = br#"{"storables":[{"id":"skinPreset","preset":"self:/Custom/Atom/Person/Skin/Left.vap"},{"id":"skinPreset","preset":"self:/Custom/Atom/Person/Skin/Right.vap"}]}"#;
        write_stored_var(
            &packages.join("Creator.Dag.1.var"),
            &[
                ("Custom/Atom/Person/Skin/Common.vap", common.as_slice()),
                ("Custom/Atom/Person/Skin/Left.vap", left.as_slice()),
                ("Custom/Atom/Person/Skin/Right.vap", right.as_slice()),
                (
                    "Custom/Atom/Person/Appearance/Root.vap",
                    root_preset.as_slice(),
                ),
                ("Custom/Atom/Person/Textures/Shared/faceD.png", b"shared"),
            ],
        );

        let root = VaMRoot::open(&temporary).unwrap();
        let report = scan_skin_library_with_report(&root).unwrap();
        let preset = report
            .presets
            .iter()
            .find(|preset| preset.label == "Root")
            .unwrap();
        assert_eq!(preset.sex, SkinSex::Female);
        assert_eq!(
            preset
                .diffuse(SkinRegion::Face)
                .as_ref()
                .unwrap()
                .read_bytes(16)
                .unwrap(),
            b"shared"
        );
        assert_eq!(report.diagnostics.skipped_presets, 0);
        fs::remove_dir_all(temporary).unwrap();
    }

    #[test]
    fn nested_reference_inheritance_preserves_first_source_order() {
        let temporary =
            std::env::temp_dir().join(format!("vkit-vam-reference-order-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temporary);
        let streaming = temporary.join("VaM_Data").join("StreamingAssets");
        let packages = temporary.join("AddonPackages");
        fs::create_dir_all(&streaming).unwrap();
        fs::create_dir_all(&packages).unwrap();
        fs::write(streaming.join("f_mb"), []).unwrap();

        let z_first = br#"{"storables":[{"id":"textures","faceDiffuseUrl":"self:/Custom/Atom/Person/Textures/Z/faceD.png"}]}"#;
        let a_second = br#"{"storables":[{"id":"textures","faceDiffuseUrl":"self:/Custom/Atom/Person/Textures/A/faceD.png"}]}"#;
        let wrapper = br#"{"storables":[{"id":"skinPreset","preset":"self:/Custom/Atom/Person/Skin/ZFirst.vap"},{"id":"skinPreset","preset":"self:/Custom/Atom/Person/Skin/ZFirst.vap"},{"id":"skinPreset","preset":"self:/Custom/Atom/Person/Skin/ASecond.vap"}]}"#;
        write_stored_var(
            &packages.join("Creator.Order.1.var"),
            &[
                ("Custom/Atom/Person/Skin/ZFirst.vap", z_first.as_slice()),
                ("Custom/Atom/Person/Skin/ASecond.vap", a_second.as_slice()),
                (
                    "Custom/Atom/Person/Appearance/Wrapper.vap",
                    wrapper.as_slice(),
                ),
                ("Custom/Atom/Person/Textures/Z/faceD.png", b"first"),
                ("Custom/Atom/Person/Textures/A/faceD.png", b"second"),
            ],
        );

        let root = VaMRoot::open(&temporary).unwrap();
        let report = scan_skin_library_with_report(&root).unwrap();
        let wrapper = report
            .presets
            .iter()
            .find(|preset| preset.label == "Wrapper")
            .unwrap();
        assert_eq!(
            wrapper
                .diffuse(SkinRegion::Face)
                .as_ref()
                .unwrap()
                .read_bytes(16)
                .unwrap(),
            b"first"
        );
        fs::remove_dir_all(temporary).unwrap();
    }

    #[test]
    fn object_reference_fields_preserve_json_source_order() {
        let value: Value = serde_json::from_str(
            r#"{"z_first":"self:/Custom/Atom/Person/Skin/ZFirst.vap","a_second":"self:/Custom/Atom/Person/Skin/ASecond.vap"}"#,
        )
        .unwrap();
        assert_eq!(
            person_preset_references(&value),
            vec![
                "self:/Custom/Atom/Person/Skin/ZFirst.vap".to_owned(),
                "self:/Custom/Atom/Person/Skin/ASecond.vap".to_owned(),
            ]
        );
    }

    #[test]
    fn broken_required_diffuse_textures_are_not_catalogued() {
        let temporary = std::env::temp_dir().join(format!(
            "vkit-vam-broken-required-textures-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temporary);
        let streaming = temporary.join("VaM_Data").join("StreamingAssets");
        let appearance = temporary
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Appearance");
        let packages = temporary.join("AddonPackages");
        fs::create_dir_all(&streaming).unwrap();
        fs::create_dir_all(&appearance).unwrap();
        fs::create_dir_all(&packages).unwrap();
        fs::write(streaming.join("f_mb"), []).unwrap();
        fs::write(
            appearance.join("Unresolved.vap"),
            br#"{"storables":[{"id":"textures","faceDiffuseUrl":"missing.package.1:/Custom/face.png"}]}"#,
        )
        .unwrap();
        let empty = br#"{"storables":[{"id":"textures","faceDiffuseUrl":"self:/Custom/Atom/Person/Textures/Empty/face.png"}]}"#;
        write_stored_var(
            &packages.join("Creator.Empty.1.var"),
            &[
                ("Custom/Atom/Person/Appearance/Empty.vap", empty.as_slice()),
                ("Custom/Atom/Person/Textures/Empty/face.png", b""),
            ],
        );

        let root = VaMRoot::open(&temporary).unwrap();
        let report = scan_skin_library_with_report(&root).unwrap();
        assert!(
            report
                .presets
                .iter()
                .all(|preset| preset.label != "Unresolved" && preset.label != "Empty")
        );
        assert_eq!(report.diagnostics.skipped_presets, 2);
        fs::remove_dir_all(temporary).unwrap();
    }

    #[test]
    fn mixed_nested_sex_references_are_unknown_independent_of_reference_order() {
        let temporary =
            std::env::temp_dir().join(format!("vkit-vam-mixed-nested-sex-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temporary);
        let streaming = temporary.join("VaM_Data").join("StreamingAssets");
        let packages = temporary.join("AddonPackages");
        fs::create_dir_all(&streaming).unwrap();
        fs::create_dir_all(&packages).unwrap();
        fs::write(streaming.join("f_mb"), []).unwrap();

        let female = br#"{"storables":[{"id":"geometry","character":"Female Custom"},{"id":"textures","faceDiffuseUrl":"self:/Custom/Atom/Person/Textures/FemaleBase/faceD.png"}]}"#;
        let male = br#"{"storables":[{"id":"geometry","character":"Male Custom"},{"id":"textures","faceDiffuseUrl":"self:/Custom/Atom/Person/Textures/MaleBase/faceD.png"}]}"#;
        let mixed = br#"{"storables":[{"id":"skinPreset","preset":"self:/Custom/Atom/Person/Skin/ZFemale.vap"},{"id":"skinPreset","preset":"self:/Custom/Atom/Person/Skin/AMale.vap"}]}"#;
        write_stored_var(
            &packages.join("Creator.Mixed.1.var"),
            &[
                ("Custom/Atom/Person/Skin/ZFemale.vap", female.as_slice()),
                ("Custom/Atom/Person/Skin/AMale.vap", male.as_slice()),
                ("Custom/Atom/Person/Appearance/Mixed.vap", mixed.as_slice()),
                (
                    "Custom/Atom/Person/Textures/FemaleBase/faceD.png",
                    b"female",
                ),
                ("Custom/Atom/Person/Textures/MaleBase/faceD.png", b"male"),
            ],
        );

        let root = VaMRoot::open(&temporary).unwrap();
        let report = scan_skin_library_with_report(&root).unwrap();
        let mixed = report
            .presets
            .iter()
            .find(|preset| preset.label == "Mixed")
            .unwrap();
        assert_eq!(mixed.sex, SkinSex::Unknown);
        fs::remove_dir_all(temporary).unwrap();
    }

    #[test]
    fn direct_and_inherited_sex_conflict_is_unknown() {
        let temporary = std::env::temp_dir().join(format!(
            "vkit-vam-direct-inherited-sex-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temporary);
        let streaming = temporary.join("VaM_Data").join("StreamingAssets");
        let packages = temporary.join("AddonPackages");
        fs::create_dir_all(&streaming).unwrap();
        fs::create_dir_all(&packages).unwrap();
        fs::write(streaming.join("f_mb"), []).unwrap();

        let male = br#"{"storables":[{"id":"geometry","character":"Male Custom"},{"id":"textures","faceDiffuseUrl":"self:/Custom/Atom/Person/Textures/MaleBase/faceD.png"}]}"#;
        let direct_female = br#"{"storables":[{"id":"geometry","character":"Female Custom"},{"id":"skinPreset","preset":"self:/Custom/Atom/Person/Skin/Male.vap"}]}"#;
        write_stored_var(
            &packages.join("Creator.DirectConflict.1.var"),
            &[
                ("Custom/Atom/Person/Skin/Male.vap", male.as_slice()),
                (
                    "Custom/Atom/Person/Appearance/DirectFemale.vap",
                    direct_female.as_slice(),
                ),
                ("Custom/Atom/Person/Textures/MaleBase/faceD.png", b"male"),
            ],
        );

        let root = VaMRoot::open(&temporary).unwrap();
        let report = scan_skin_library_with_report(&root).unwrap();
        let direct = report
            .presets
            .iter()
            .find(|preset| preset.label == "DirectFemale")
            .unwrap();
        assert_eq!(direct.sex, SkinSex::Unknown);
        fs::remove_dir_all(temporary).unwrap();
    }

    #[test]
    fn explicit_geometry_sex_ignores_opposite_sex_clothing_and_hair_paths() {
        let value: Value = serde_json::from_str(
            r#"{"storables":[{"id":"geometry","character":"Female Custom","clothing":[{"id":"Creator:/Custom/Clothing/Male/EyeShadow.vam"}],"hair":[{"id":"Creator:/Custom/Hair/Male/Short.vam"}]},{"id":"textures","faceDiffuseUrl":"Creator:/Custom/Atom/Person/Textures/Character/faceD.png"}]}"#,
        )
        .unwrap();
        assert_eq!(
            classify_skin_sex(
                &value,
                &AssetLocator::UnresolvedVaMUrl("preset".to_owned()),
                None,
                None,
            ),
            SkinSex::Female
        );
    }

    #[test]
    fn sex_classification_is_conflict_safe_and_filtering_is_explicit() {
        let female_value: Value = serde_json::from_str(
            r#"{"storables":[{"id":"geometry","character":"Female Custom"}]}"#,
        )
        .unwrap();
        let male_value: Value =
            serde_json::from_str(r#"{"storables":[{"id":"geometry","character":"Male Custom"}]}"#)
                .unwrap();
        let conflict_value: Value = serde_json::from_str(
            r#"{"storables":[{"id":"geometry","character":"Female Custom"},{"id":"textures","faceDiffuseUrl":"Creator:/Custom/Atom/Person/Textures/MaleBase/faceD.png"}]}"#,
        )
        .unwrap();
        let source = AssetLocator::UnresolvedVaMUrl("preset".to_owned());
        assert_eq!(
            classify_skin_sex(&female_value, &source, None, None),
            SkinSex::Female
        );
        assert_eq!(
            classify_skin_sex(&male_value, &source, None, None),
            SkinSex::Male
        );
        assert_eq!(
            classify_skin_sex(&conflict_value, &source, None, None),
            SkinSex::Unknown
        );

        let mut female = SkinPreset {
            stable_id: "female".to_owned(),
            label: "Female".to_owned(),
            source: source.clone(),
            sex: SkinSex::Female,
            textures: SkinTextureSet::default(),
            auxiliary: SkinAuxiliary::default(),
        };
        let mut male = female.clone();
        male.stable_id = "male".to_owned();
        male.sex = SkinSex::Male;
        female.label = "Female".to_owned();
        let mut unknown = female.clone();
        unknown.stable_id = "unknown".to_owned();
        unknown.sex = SkinSex::Unknown;
        let presets = vec![female, male, unknown];
        assert_eq!(
            filter_skin_presets_by_sex(&presets, SkinSex::Female, false).len(),
            1
        );
        assert_eq!(
            filter_skin_presets_by_sex(&presets, SkinSex::Female, true).len(),
            2
        );
        assert_eq!(
            filter_skin_presets_by_sex(&presets, SkinSex::Unknown, false).len(),
            3
        );
    }

    #[test]
    fn scans_and_reads_self_referenced_var_texture() {
        let temporary =
            std::env::temp_dir().join(format!("vkit-vam-var-skin-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temporary);
        let streaming = temporary.join("VaM_Data").join("StreamingAssets");
        let packages = temporary.join("AddonPackages");
        fs::create_dir_all(&streaming).unwrap();
        fs::create_dir_all(&packages).unwrap();
        fs::write(streaming.join("f_mb"), []).unwrap();
        let vap = br#"{"storables":[{"id":"textures","faceDiffuseUrl":"self:/Custom/Atom/Person/Textures/Test/faceD.png"}]}"#;
        let upper_vap = br#"{"storables":[{"id":"textures","faceDiffuseUrl":"SELF:/Custom/Atom/Person/Textures/Test/faceD.png"}]}"#;
        write_stored_var(
            &packages.join("Creator.Test.1.var"),
            &[
                ("Custom/Atom/Person/Appearance/Test.vap", vap.as_slice()),
                (
                    "Custom/Atom/Person/Appearance/Upper.vap",
                    upper_vap.as_slice(),
                ),
                ("Custom/Atom/Person/Textures/Test/faceD.png", b"image"),
            ],
        );
        let root = VaMRoot::open(&temporary).unwrap();
        let report = scan_skin_library_with_report(&root).unwrap();
        assert_eq!(report.diagnostics.skipped_presets, 0);
        let texture = report
            .presets
            .iter()
            .find(|preset| preset.label == "Test")
            .and_then(|preset| preset.diffuse(SkinRegion::Face))
            .unwrap();
        assert!(matches!(texture, AssetLocator::VarEntry { .. }));
        assert_eq!(texture.read_bytes(16).unwrap(), b"image");
        assert!(texture.read_bytes(4).is_err());
        assert!(
            report
                .presets
                .iter()
                .find(|preset| preset.label == "Upper")
                .and_then(|preset| preset.diffuse(SkinRegion::Face))
                .is_some_and(|texture| matches!(texture, AssetLocator::VarEntry { .. }))
        );
        fs::remove_dir_all(temporary).unwrap();
    }

    #[test]
    #[ignore = "reads the user's VaM install"]
    fn real_skins_bring_their_bodies() {
        let Some(root) = std::env::var_os("VKIT_VAM_ROOT")
            .map(std::path::PathBuf::from)
            .and_then(|path| super::super::VaMRoot::open(&path).ok())
        else {
            eprintln!("set VKIT_VAM_ROOT to run this");
            return;
        };
        let started = std::time::Instant::now();
        let report = scan_skin_library_with_report(&root).expect("skin scan");
        println!(
            "{} skins in {:.2}s",
            report.presets.len(),
            started.elapsed().as_secs_f64()
        );
        let mut per_region: std::collections::BTreeMap<&str, usize> =
            std::collections::BTreeMap::new();
        let mut per_channel: std::collections::BTreeMap<&str, usize> =
            std::collections::BTreeMap::new();
        for preset in &report.presets {
            for (region, channel, _) in preset.textures.iter() {
                *per_region.entry(region.vam_prefix()).or_default() += 1;
                *per_channel.entry(channel.vam_suffix()).or_default() += 1;
            }
        }
        println!("by region: {per_region:?}");
        println!("by channel: {per_channel:?}");
        let total: usize = per_region.values().sum();
        println!("{total} textures across {} skins", report.presets.len());
        assert!(
            per_region.get("limbs").copied().unwrap_or_default() > 0,
            "a library with real skins has limbs textures, and they used to be dropped"
        );
    }

    #[test]
    fn drive_urls_latest_packages_and_unresolved_fallback_are_classified_safely() {
        assert!(is_windows_absolute_url("F:/textures/face.png"));
        assert!(is_windows_absolute_url("c:/textures/face.png"));
        assert!(!is_windows_absolute_url("Creator.Package.1:/face.png"));

        let exact = BTreeMap::from([
            ("creator.skin.1".to_owned(), vec![0]),
            ("creator.skin.12".to_owned(), vec![1]),
            ("creator.skin.3".to_owned(), vec![2]),
        ]);
        let uids = ["Creator.Skin.1", "Creator.Skin.12", "Creator.Skin.3"];
        assert_eq!(
            select_package_index("creator.skin.latest", &exact, |index| uids[index]),
            Some(1)
        );

        let temporary =
            std::env::temp_dir().join(format!("vkit-vam-url-resolution-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temporary);
        fs::create_dir_all(temporary.join("VaM_Data").join("StreamingAssets")).unwrap();
        fs::write(
            temporary
                .join("VaM_Data")
                .join("StreamingAssets")
                .join("f_mb"),
            [],
        )
        .unwrap();
        let root = VaMRoot::open(&temporary).unwrap();
        let packages = ScanPackages {
            archives: Vec::new(),
            archive_by_uid: BTreeMap::new(),
            expanded: Vec::new(),
            expanded_by_uid: BTreeMap::new(),
            cache_textures: Vec::new(),
        };
        assert_eq!(
            packages.resolve_url(
                "missing.package.latest:/Custom/face.png",
                PresetContainer::Local,
                &root,
            ),
            AssetLocator::UnresolvedVaMUrl("missing.package.latest:/Custom/face.png".to_owned())
        );
        assert_eq!(
            packages.resolve_url("F:/does/not/exist/face.png", PresetContainer::Local, &root,),
            AssetLocator::UnresolvedVaMUrl("F:/does/not/exist/face.png".to_owned())
        );
        fs::remove_dir_all(temporary).unwrap();
    }
}

#[cfg(test)]
mod full_body_texture_tests {
    use super::*;

    #[test]
    fn every_region_and_channel_survives_the_parse() {
        let mut set = SkinTextureSet::default();
        for region in SkinRegion::ALL {
            for channel in SkinTextureChannel::ALL {
                set.insert(
                    region,
                    channel,
                    AssetLocator::UnresolvedVaMUrl(format!(
                        "{}{}",
                        region.vam_prefix(),
                        channel.vam_suffix()
                    )),
                );
            }
        }
        assert_eq!(set.len(), 24, "four regions times six channels");
        for region in SkinRegion::ALL {
            for channel in SkinTextureChannel::ALL {
                assert!(
                    set.get(region, channel).is_some(),
                    "{}{} is missing",
                    region.vam_prefix(),
                    channel.vam_suffix()
                );
            }
        }
    }

    #[test]
    fn the_keys_match_what_vam_writes() {
        let key = |region: SkinRegion, channel: SkinTextureChannel| {
            format!("{}{}Url", region.vam_prefix(), channel.vam_suffix())
        };
        assert_eq!(
            key(SkinRegion::Limbs, SkinTextureChannel::Diffuse),
            "limbsDiffuseUrl"
        );
        assert_eq!(
            key(SkinRegion::Genitals, SkinTextureChannel::Normal),
            "genitalsNormalUrl"
        );
        assert_eq!(
            key(SkinRegion::Torso, SkinTextureChannel::Decal),
            "torsoDecalUrl"
        );
        assert_eq!(
            key(SkinRegion::Face, SkinTextureChannel::Detail),
            "faceDetailUrl"
        );
    }

    #[test]
    fn inheriting_fills_the_slots_a_wrapper_left_alone() {
        let locator = |name: &str| AssetLocator::UnresolvedVaMUrl(name.to_owned());
        let mut wrapper = SkinTextureSet::default();
        wrapper.insert(
            SkinRegion::Face,
            SkinTextureChannel::Diffuse,
            locator("wrapper-face"),
        );
        let mut base = SkinTextureSet::default();
        base.insert(
            SkinRegion::Face,
            SkinTextureChannel::Diffuse,
            locator("base-face"),
        );
        base.insert(
            SkinRegion::Limbs,
            SkinTextureChannel::Diffuse,
            locator("base-limbs"),
        );

        wrapper.fill_missing_from(&base);
        assert_eq!(
            wrapper.get(SkinRegion::Face, SkinTextureChannel::Diffuse),
            Some(&locator("wrapper-face")),
            "the wrapper's own texture wins"
        );
        assert_eq!(
            wrapper.get(SkinRegion::Limbs, SkinTextureChannel::Diffuse),
            Some(&locator("base-limbs")),
            "and what it did not name falls through"
        );
    }
}
