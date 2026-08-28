use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

use serde_json::Value;
use vkit_core::vam::{SkinSex, list_var_entries, read_var_entry_bytes};

const MAX_SOURCE_FILES: usize = 20_000;
const MAX_PRESET_BYTES: u64 = 16 * 1024 * 1024;
const MAX_MORPH_ASSET_BYTES: usize = 20 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VaMEditSourceKind {
    MorphPair,
    AppearancePreset,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EditSourceFilter {
    #[default]
    All,
    Looks,
    Morphs,
}

impl EditSourceFilter {
    pub const ALL: [Self; 3] = [Self::All, Self::Looks, Self::Morphs];

    #[must_use]
    pub const fn admits(self, kind: VaMEditSourceKind) -> bool {
        match self {
            Self::All => true,
            Self::Looks => matches!(kind, VaMEditSourceKind::AppearancePreset),
            Self::Morphs => matches!(kind, VaMEditSourceKind::MorphPair),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaMEditSource {
    pub stable_id: String,
    pub label: String,
    pub path: PathBuf,
    pub sex: Option<SkinSex>,
    pub kind: VaMEditSourceKind,

    pub missing_morphs: u32,
    pub morph_refs: u32,
}

impl VaMEditSource {
    #[must_use]
    pub const fn resolves_nothing(&self) -> bool {
        self.morph_refs > 0 && self.missing_morphs >= self.morph_refs
    }
}

#[derive(Clone, Debug, Default)]
pub struct VaMEditSourceCatalog {
    pub sources: Vec<VaMEditSource>,
    pub warnings: Vec<String>,

    pub groups: Vec<String>,
    pub regions: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct AppearanceMorphValue {
    pub uid: String,
    pub name: String,
    pub value: f32,
}

#[derive(Clone, Debug)]
pub struct AppearanceRecipe {
    #[cfg_attr(not(test), allow(dead_code, reason = "parsed metadata, test-asserted"))]
    pub label: String,
    pub sex: Option<SkinSex>,
    pub morphs: Vec<AppearanceMorphValue>,
}

#[derive(Clone, Debug)]
pub enum ResolvedMorphPairAsset {
    Loose(PathBuf),
    Packed(PackedMorphPairAsset),
}

#[derive(Clone, Debug)]
pub struct PackedMorphPairAsset {
    pub route_path: PathBuf,
    pub stable_id: String,
    pub vmi_bytes: Vec<u8>,
    pub vmb_bytes: Vec<u8>,
}

pub fn scan_edit_sources(vam_root: &Path) -> Result<VaMEditSourceCatalog, String> {
    let mut catalog = VaMEditSourceCatalog::default();
    let mut observed_groups = BTreeSet::new();
    let mut observed_regions = BTreeSet::new();
    for (folder, sex) in [
        (
            vam_root
                .join("Custom")
                .join("Atom")
                .join("Person")
                .join("Morphs")
                .join("female"),
            SkinSex::Female,
        ),
        (
            vam_root
                .join("Custom")
                .join("Atom")
                .join("Person")
                .join("Morphs")
                .join("male"),
            SkinSex::Male,
        ),
    ] {
        collect_files(
            &folder,
            "vmi",
            &mut catalog.warnings,
            |path| {
                if !path.with_extension("vmb").is_file() {
                    return None;
                }

                if let Ok(bytes) = fs::read(path)
                    && let Ok(document) = vkit_core::vam::parse_vmi(&bytes)
                {
                    for (value, sink) in [
                        (document.group.as_deref(), &mut observed_groups),
                        (document.region.as_deref(), &mut observed_regions),
                    ] {
                        if let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) {
                            sink.insert(value.to_owned());
                        }
                    }
                }
                Some(source_from_path(
                    path,
                    Some(sex),
                    VaMEditSourceKind::MorphPair,
                ))
            },
            &mut catalog.sources,
        );
    }

    let mut archive_entries: BTreeMap<PathBuf, Option<BTreeSet<String>>> = BTreeMap::new();
    for folder in [
        vam_root
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Appearance"),
        vam_root.join("Saves").join("Person").join("appearance"),
    ] {
        collect_files(
            &folder,
            "vap",
            &mut catalog.warnings,
            |path| {
                let recipe = parse_appearance_recipe(path).ok();
                let sex = recipe.as_ref().and_then(|recipe| recipe.sex);
                let mut source = source_from_path(path, sex, VaMEditSourceKind::AppearancePreset);
                if let Some(recipe) = recipe.as_ref() {
                    source.morph_refs = recipe.morphs.len() as u32;
                    source.missing_morphs = recipe
                        .morphs
                        .iter()
                        .filter(|morph| {
                            !morph_ref_available(vam_root, &morph.uid, &mut archive_entries)
                        })
                        .count() as u32;
                }
                Some(source)
            },
            &mut catalog.sources,
        );
    }

    catalog.sources.sort_by(|left, right| {
        left.resolves_nothing()
            .cmp(&right.resolves_nothing())
            .then_with(|| {
                left.label
                    .to_ascii_lowercase()
                    .cmp(&right.label.to_ascii_lowercase())
            })
            .then_with(|| left.stable_id.cmp(&right.stable_id))
    });
    catalog
        .sources
        .dedup_by(|left, right| left.stable_id == right.stable_id);

    let mut seen = std::collections::HashSet::new();
    catalog.sources.retain(|source| {
        seen.insert(format!(
            "{:?}\u{1}{:?}\u{1}{}",
            source.kind,
            source.sex,
            source.label.to_ascii_lowercase()
        ))
    });
    catalog.groups = observed_groups.into_iter().collect();
    catalog.regions = observed_regions.into_iter().collect();
    Ok(catalog)
}

pub fn parse_appearance_recipe(path: &Path) -> Result<AppearanceRecipe, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Cannot read appearance preset {}: {error}", path.display()))?;
    if metadata.len() > MAX_PRESET_BYTES {
        return Err(format!(
            "Appearance preset is larger than {} MiB: {}",
            MAX_PRESET_BYTES / (1024 * 1024),
            path.display()
        ));
    }
    let bytes = fs::read(path)
        .map_err(|error| format!("Cannot read appearance preset {}: {error}", path.display()))?;
    let root: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Invalid appearance preset {}: {error}", path.display()))?;
    let storables = root
        .get("storables")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Appearance preset has no storables: {}", path.display()))?;
    let geometry = storables
        .iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some("geometry"))
        .ok_or_else(|| {
            format!(
                "Appearance preset has no geometry storable: {}",
                path.display()
            )
        })?;
    let character = geometry
        .get("character")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let sex = infer_appearance_sex(character, storables);
    let morphs = geometry
        .get("morphs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let uid = entry.get("uid").and_then(Value::as_str)?.trim();
            if uid.is_empty() {
                return None;
            }
            let value = parse_number(entry.get("value")?)?;
            if !value.is_finite() || value.abs() <= 1.0e-8 {
                return None;
            }
            let name = entry
                .get("name")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(uid);
            Some(AppearanceMorphValue {
                uid: uid.to_owned(),
                name: name.to_owned(),
                value,
            })
        })
        .collect();
    let label = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Appearance")
        .trim_start_matches("Preset_")
        .to_owned();
    Ok(AppearanceRecipe { label, sex, morphs })
}

pub fn resolve_loose_morph_path(vam_root: &Path, uid: &str) -> Option<PathBuf> {
    let normalized = uid.replace('\\', "/");
    let relative = normalized
        .split_once(":/")
        .map(|(_, tail)| tail)
        .unwrap_or(normalized.as_str())
        .trim_start_matches('/');
    if !relative.to_ascii_lowercase().ends_with(".vmi") {
        return None;
    }
    let path = vam_root.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
    (path.is_file() && path.with_extension("vmb").is_file()).then_some(path)
}

pub fn resolve_morph_pair_asset(
    vam_root: &Path,
    uid: &str,
) -> Result<Option<ResolvedMorphPairAsset>, String> {
    let normalized = uid.replace('\\', "/");
    let package_and_entry = normalized.split_once(":/");
    let relative = package_and_entry
        .map(|(_, entry)| entry)
        .unwrap_or(normalized.as_str())
        .trim_start_matches('/');
    if !relative.to_ascii_lowercase().ends_with(".vmi") {
        return Ok(None);
    }

    if let Some((package, entry)) = package_and_entry
        && !package.eq_ignore_ascii_case("self")
    {
        let addon_packages = vam_root.join("AddonPackages");
        if let Some(archive) = resolve_package_archive(&addon_packages, package) {
            let vmb_entry = Path::new(entry)
                .with_extension("vmb")
                .to_string_lossy()
                .replace('\\', "/");
            let vmi_bytes = read_var_entry_bytes(&archive, entry, MAX_MORPH_ASSET_BYTES)
                .map_err(|error| error.to_string())?;
            let vmb_bytes = read_var_entry_bytes(&archive, &vmb_entry, MAX_MORPH_ASSET_BYTES)
                .map_err(|error| error.to_string())?;
            return Ok(Some(ResolvedMorphPairAsset::Packed(PackedMorphPairAsset {
                route_path: PathBuf::from(entry),
                stable_id: normalized,
                vmi_bytes,
                vmb_bytes,
            })));
        }
        if let Some(package_directory) = resolve_expanded_package(&addon_packages, package)
            && let Some(relative_path) = safe_package_relative_path(entry)
        {
            let vmi_path = package_directory.join(relative_path);
            if vmi_path.is_file() && vmi_path.with_extension("vmb").is_file() {
                return Ok(Some(ResolvedMorphPairAsset::Loose(vmi_path)));
            }
        }
    }

    Ok(resolve_loose_morph_path(vam_root, uid).map(ResolvedMorphPairAsset::Loose))
}

fn morph_ref_available(
    vam_root: &Path,
    uid: &str,
    archive_entries: &mut BTreeMap<PathBuf, Option<BTreeSet<String>>>,
) -> bool {
    let normalized = uid.replace('\\', "/");
    let package_and_entry = normalized.split_once(":/");
    let relative = package_and_entry
        .map(|(_, entry)| entry)
        .unwrap_or(normalized.as_str())
        .trim_start_matches('/');
    if !relative.to_ascii_lowercase().ends_with(".vmi") {
        return true;
    }

    if let Some((package, entry)) = package_and_entry
        && !package.eq_ignore_ascii_case("self")
    {
        let addon_packages = vam_root.join("AddonPackages");
        if let Some(archive) = resolve_package_archive(&addon_packages, package) {
            let names = archive_entries
                .entry(archive.clone())
                .or_insert_with(|| list_var_entries(&archive).ok().map(collect_entry_names));
            if let Some(names) = names {
                let vmi_entry = entry.replace('\\', "/").to_ascii_lowercase();
                let vmb_entry = Path::new(entry)
                    .with_extension("vmb")
                    .to_string_lossy()
                    .replace('\\', "/")
                    .to_ascii_lowercase();
                if names.contains(&vmi_entry) && names.contains(&vmb_entry) {
                    return true;
                }
            }
        }
        if let Some(package_directory) = resolve_expanded_package(&addon_packages, package)
            && let Some(relative_path) = safe_package_relative_path(entry)
        {
            let vmi_path = package_directory.join(relative_path);
            if vmi_path.is_file() && vmi_path.with_extension("vmb").is_file() {
                return true;
            }
        }
    }

    resolve_loose_morph_path(vam_root, uid).is_some()
}

fn collect_entry_names(entries: Vec<String>) -> BTreeSet<String> {
    entries
        .into_iter()
        .map(|name| name.replace('\\', "/").to_ascii_lowercase())
        .collect()
}

fn resolve_package_archive(addon_packages: &Path, package: &str) -> Option<PathBuf> {
    let exact = addon_packages.join(format!("{package}.var"));
    if exact.is_file() {
        return Some(exact);
    }
    resolve_latest_package(addon_packages, package, false)
}

fn resolve_expanded_package(addon_packages: &Path, package: &str) -> Option<PathBuf> {
    let exact = addon_packages.join(package);
    if exact.is_dir() {
        return Some(exact);
    }
    resolve_latest_package(addon_packages, package, true)
}

fn resolve_latest_package(
    addon_packages: &Path,
    package: &str,
    directory: bool,
) -> Option<PathBuf> {
    let base = match package.rsplit_once('.') {
        Some((base, version))
            if version.eq_ignore_ascii_case("latest") || version.parse::<u64>().is_ok() =>
        {
            base
        }
        _ => return None,
    };
    let prefix = format!("{base}.").to_ascii_lowercase();
    fs::read_dir(addon_packages)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if directory != path.is_dir() {
                return None;
            }
            if !directory
                && !path
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| value.eq_ignore_ascii_case("var"))
            {
                return None;
            }
            let stem = if directory {
                path.file_name()?.to_str()?
            } else {
                path.file_stem()?.to_str()?
            };
            let suffix = stem.to_ascii_lowercase().strip_prefix(&prefix)?.to_owned();
            let version = suffix.parse::<u64>().ok()?;
            Some((version, path))
        })
        .max_by_key(|(version, _)| *version)
        .map(|(_, path)| path)
}

fn safe_package_relative_path(entry: &str) -> Option<PathBuf> {
    let path = Path::new(entry.trim_start_matches(['/', '\\']));
    path.components()
        .all(|component| matches!(component, Component::Normal(_)))
        .then(|| path.to_path_buf())
}

pub fn morph_identity(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn morph_identity_candidates(value: &str) -> Vec<String> {
    let normalized = value.replace('\\', "/");
    let relative = normalized
        .split_once(":/")
        .map(|(_, entry)| entry)
        .unwrap_or(normalized.as_str());
    let mut candidates = vec![morph_identity(value)];
    if let Some(stem) = Path::new(relative)
        .file_stem()
        .and_then(|stem| stem.to_str())
    {
        candidates.push(morph_identity(stem));
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

#[must_use]
pub fn source_from_path(
    path: &Path,
    sex: Option<SkinSex>,
    kind: VaMEditSourceKind,
) -> VaMEditSource {
    let label = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Unnamed")
        .trim_start_matches("Preset_")
        .to_owned();
    VaMEditSource {
        stable_id: format!(
            "{}:{}",
            match kind {
                VaMEditSourceKind::MorphPair => "morph",
                VaMEditSourceKind::AppearancePreset => "appearance",
            },
            path.to_string_lossy()
        ),
        label,
        path: path.to_path_buf(),
        sex,
        kind,
        missing_morphs: 0,
        morph_refs: 0,
    }
}

fn collect_files(
    root: &Path,
    extension: &str,
    warnings: &mut Vec<String>,
    mut map: impl FnMut(&Path) -> Option<VaMEditSource>,
    output: &mut Vec<VaMEditSource>,
) {
    if !root.is_dir() {
        return;
    }
    let mut pending = vec![root.to_path_buf()];
    let mut visited = 0usize;
    while let Some(folder) = pending.pop() {
        let entries = match fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(error) => {
                warnings.push(format!("Cannot index {}: {error}", folder.display()));
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case(extension))
            {
                if let Some(source) = map(&path) {
                    output.push(source);
                }
                visited += 1;
                if visited >= MAX_SOURCE_FILES {
                    warnings.push(format!(
                        "Source index stopped after {MAX_SOURCE_FILES} files under {}",
                        root.display()
                    ));
                    return;
                }
            }
        }
    }
}

fn parse_number(value: &Value) -> Option<f32> {
    value
        .as_f64()
        .map(|number| number as f32)
        .or_else(|| value.as_str()?.parse::<f32>().ok())
}

fn infer_appearance_sex(character: &str, storables: &[Value]) -> Option<SkinSex> {
    let ids = storables
        .iter()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();

    let male = ids
        .iter()
        .any(|id| id.starts_with("maleanatomy") || id == "pectoralcontrol");
    let female = ids
        .iter()
        .any(|id| id.starts_with("femaleanatomy") || id == "breastcontrol");
    match (male, female) {
        (true, false) => return Some(SkinSex::Male),
        (false, true) => return Some(SkinSex::Female),
        _ => {}
    }
    let character = character.to_ascii_lowercase();
    if character.contains("female") {
        Some(SkinSex::Female)
    } else if character.contains("male") {
        Some(SkinSex::Male)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

    fn test_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "vkit-{label}-{}-{}",
            std::process::id(),
            TEST_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ))
    }

    #[test]
    fn a_morph_exported_here_comes_back_as_a_source_you_can_load() {
        use vkit_core::vam::{GeometrySex, VaMRoot};

        let root = test_root("export-round-trip");
        let folder = root
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Morphs")
            .join("female")
            .join("Vkit");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("Aria.vmi"), b"{}").unwrap();
        fs::write(folder.join("Aria.vmb"), [0_u8]).unwrap();

        fs::write(folder.join("Halfway.vmi"), b"{}").unwrap();

        if let Ok(opened) = VaMRoot::open(&root) {
            let written = opened.morph_output_directory(GeometrySex::Female);
            let tail: Vec<_> = written
                .components()
                .rev()
                .take(5)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let wanted: Vec<_> = Path::new("Custom/Atom/Person/Morphs/female")
                .components()
                .collect();
            assert_eq!(
                tail, wanted,
                "the exporter writes somewhere the scan does not look",
            );
        }

        let catalog = scan_edit_sources(&root).expect("scan");
        let found: Vec<&VaMEditSource> = catalog
            .sources
            .iter()
            .filter(|source| source.kind == VaMEditSourceKind::MorphPair)
            .collect();
        assert_eq!(found.len(), 1, "{:?}", catalog.sources);
        assert_eq!(found[0].sex, Some(SkinSex::Female), "read from the folder");
        assert!(
            !found[0].resolves_nothing(),
            "a morph pair references no morphs, so it can never resolve to nothing",
        );
        assert!(
            EditSourceFilter::All.admits(found[0].kind)
                && EditSourceFilter::Morphs.admits(found[0].kind)
                && !EditSourceFilter::Looks.admits(found[0].kind),
            "the list filter has to be able to show it",
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_female_anatomy_storable_is_not_read_as_male() {
        let female = serde_json::json!([
            {"id": "FemaleAnatomy"},
            {"id": "BreastControl"},
        ]);
        assert_eq!(
            infer_appearance_sex("Lexi", female.as_array().unwrap()),
            Some(SkinSex::Female)
        );

        let male = serde_json::json!([{"id": "MaleAnatomy"}, {"id": "PectoralControl"}]);
        assert_eq!(
            infer_appearance_sex("Lee", male.as_array().unwrap()),
            Some(SkinSex::Male)
        );
    }

    #[test]
    fn appearance_parser_reads_geometry_morphs_and_infers_sex() {
        let root = test_root("vap");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("Preset_Test.vap");
        fs::write(
            &path,
            br#"{"storables":[{"id":"geometry","character":"Lee","morphs":[{"uid":"Head","name":"Head","value":"0.75"},{"uid":"Zero","value":"0"}]},{"id":"MaleAnatomy","enabled":"true"}]}"#,
        )
        .unwrap();
        let recipe = parse_appearance_recipe(&path).unwrap();
        assert_eq!(recipe.label, "Test");
        assert_eq!(recipe.sex, Some(SkinSex::Male));
        assert_eq!(recipe.morphs.len(), 1);
        assert_eq!(recipe.morphs[0].value, 0.75);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn female_character_name_is_not_misclassified_as_male() {
        assert_eq!(
            infer_appearance_sex("Female Custom", &[]),
            Some(SkinSex::Female)
        );
    }

    #[test]
    fn packaged_morph_identity_includes_the_file_stem() {
        let identities = morph_identity_candidates(
            "Author.Look.1:/Custom/Atom/Person/Morphs/female/Head/Face Shape.vmi",
        );
        assert!(identities.contains(&"faceshape".to_owned()));
    }

    #[test]
    fn latest_package_resolution_uses_highest_numeric_version_and_expanded_fallback() {
        let root = test_root("latest-package");
        let packages = root.join("AddonPackages");
        fs::create_dir_all(&packages).unwrap();
        fs::write(packages.join("Creator.Shape.2.var"), []).unwrap();
        fs::write(packages.join("Creator.Shape.11.var"), []).unwrap();
        fs::write(packages.join("Creator.Shape.beta.var"), []).unwrap();
        assert_eq!(
            resolve_package_archive(&packages, "Creator.Shape.latest")
                .unwrap()
                .file_name()
                .and_then(|value| value.to_str()),
            Some("Creator.Shape.11.var")
        );

        assert_eq!(
            resolve_package_archive(&packages, "Creator.Shape.7")
                .unwrap()
                .file_name()
                .and_then(|value| value.to_str()),
            Some("Creator.Shape.11.var")
        );

        assert_eq!(
            resolve_package_archive(&packages, "Creator.Shape.2")
                .unwrap()
                .file_name()
                .and_then(|value| value.to_str()),
            Some("Creator.Shape.2.var")
        );

        fs::create_dir_all(packages.join("Creator.Expanded.3")).unwrap();
        fs::create_dir_all(packages.join("Creator.Expanded.8")).unwrap();
        assert_eq!(
            resolve_expanded_package(&packages, "Creator.Expanded.latest")
                .unwrap()
                .file_name()
                .and_then(|value| value.to_str()),
            Some("Creator.Expanded.8")
        );
        assert!(safe_package_relative_path("../escape.vmi").is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn only_a_look_that_resolves_nothing_is_marked_and_sunk() {
        let root = test_root("edit-source-availability");
        let _ = fs::remove_dir_all(&root);
        let morphs = root
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Morphs")
            .join("female");
        let looks = root
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Appearance");
        fs::create_dir_all(&morphs).unwrap();
        fs::create_dir_all(&looks).unwrap();

        fs::write(morphs.join("Present.vmi"), b"{}").unwrap();
        fs::write(morphs.join("Present.vmb"), b"x").unwrap();
        fs::write(
            looks.join("Preset_Ready.vap"),
            br#"{"storables":[{"id":"geometry","morphs":[{"uid":"Custom/Atom/Person/Morphs/female/Present.vmi","name":"Present","value":"1"}]}]}"#,
        )
        .unwrap();

        fs::write(
            looks.join("Preset_Partial.vap"),
            br#"{"storables":[{"id":"geometry","morphs":[{"uid":"Custom/Atom/Person/Morphs/female/Present.vmi","name":"Present","value":"1"},{"uid":"Custom/Atom/Person/Morphs/female/Gone.vmi","name":"Gone","value":"1"}]}]}"#,
        )
        .unwrap();

        fs::write(
            looks.join("Preset_Empty.vap"),
            br#"{"storables":[{"id":"geometry","morphs":[{"uid":"Custom/Atom/Person/Morphs/female/Gone.vmi","name":"Gone","value":"1"}]}]}"#,
        )
        .unwrap();

        let catalog = scan_edit_sources(&root).unwrap();
        let look = |label: &str| {
            catalog
                .sources
                .iter()
                .find(|source| source.label == label)
                .unwrap_or_else(|| panic!("{label} indexed"))
        };
        assert_eq!(
            look("Ready").missing_morphs,
            0,
            "present morph resolves loose"
        );
        assert_eq!(
            look("Partial").missing_morphs,
            1,
            "only the absent morph is counted"
        );
        assert!(
            !look("Partial").resolves_nothing(),
            "a look that still carries a face is never marked"
        );
        assert!(
            look("Empty").resolves_nothing(),
            "a look with nothing left to load is marked"
        );

        let pair = catalog
            .sources
            .iter()
            .find(|source| source.kind == VaMEditSourceKind::MorphPair)
            .expect("loose pair indexed");
        assert_eq!(pair.missing_morphs, 0);
        assert!(!pair.resolves_nothing());

        let empty_index = catalog
            .sources
            .iter()
            .position(|source| source.label == "Empty")
            .unwrap();
        let last_usable = catalog
            .sources
            .iter()
            .rposition(|source| !source.resolves_nothing())
            .unwrap();
        assert!(
            empty_index > last_usable,
            "a look that resolves nothing must sink below the usable sources"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn identically_named_morphs_from_several_locations_collapse_to_one() {
        let root = test_root("edit-source-dedup");
        let _ = fs::remove_dir_all(&root);
        let base = root
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Morphs")
            .join("female");
        for package in ["PackA", "PackB"] {
            let dir = base.join(package);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("Cheek.vmi"), b"{}").unwrap();
            fs::write(dir.join("Cheek.vmb"), b"x").unwrap();
        }

        fs::write(base.join("Nose.vmi"), b"{}").unwrap();
        fs::write(base.join("Nose.vmb"), b"x").unwrap();

        let catalog = scan_edit_sources(&root).unwrap();
        let cheeks = catalog
            .sources
            .iter()
            .filter(|source| source.label == "Cheek")
            .count();
        assert_eq!(cheeks, 1, "the duplicated morph collapses to one row");
        assert_eq!(
            catalog
                .sources
                .iter()
                .filter(|source| source.label == "Nose")
                .count(),
            1,
            "a distinct morph is untouched"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn package_and_loose_uids_resolve_to_the_same_owned_path() {
        let root = test_root("vmi");
        let pair = root
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Morphs")
            .join("female")
            .join("Head");
        fs::create_dir_all(&pair).unwrap();
        fs::write(pair.join("Shape.vmi"), b"{}").unwrap();
        fs::write(pair.join("Shape.vmb"), b"x").unwrap();
        let loose = "Custom/Atom/Person/Morphs/female/Head/Shape.vmi";
        let packaged = format!("Author.Package.1:/{loose}");
        assert_eq!(
            resolve_loose_morph_path(&root, loose),
            resolve_loose_morph_path(&root, &packaged)
        );
        fs::remove_dir_all(root).unwrap();
    }
}
