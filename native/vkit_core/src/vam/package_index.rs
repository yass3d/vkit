use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use serde::{Deserialize, Serialize};

use super::skin::list_var_entries;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum PackageAssetKind {
    Pose,

    Appearance,

    Morph,
    Skin,
    Hair,
    Clothing,
    Texture,

    Scene,

    OtherPreset,
}

impl PackageAssetKind {
    pub const ALL: [Self; 9] = [
        Self::Pose,
        Self::Appearance,
        Self::Morph,
        Self::Skin,
        Self::Hair,
        Self::Clothing,
        Self::Texture,
        Self::Scene,
        Self::OtherPreset,
    ];
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct PackageAsset {
    pub package: u32,

    pub entry: String,
    pub kind: PackageAssetKind,
}

impl PackageAsset {
    pub fn display_name(&self) -> &str {
        let file = self.entry.rsplit('/').next().unwrap_or(self.entry.as_str());
        file.rsplit_once('.').map_or(file, |(stem, _)| stem)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct PackageRef {
    pub path: PathBuf,

    pub uid: String,
    len: u64,
    modified_ms: u64,
}

impl PackageRef {
    fn matches(&self, path: &Path) -> bool {
        stamp(path)
            .is_some_and(|(len, modified_ms)| len == self.len && modified_ms == self.modified_ms)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PackageIndex {
    pub packages: Vec<PackageRef>,
    pub assets: Vec<PackageAsset>,

    pub unreadable: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageIndexProgress {
    pub done: usize,
    pub total: usize,

    pub reused: usize,
}

impl PackageIndex {
    pub fn build(
        addon_packages: &Path,
        previous: Option<&Self>,
        mut progress: impl FnMut(PackageIndexProgress),
    ) -> Self {
        let mut paths = archive_paths(addon_packages);
        paths.sort();
        let cached = previous.map(index_by_path).unwrap_or_default();

        let mut index = Self::default();
        let total = paths.len();
        let mut reused = 0;
        for (done, path) in paths.into_iter().enumerate() {
            progress(PackageIndexProgress {
                done,
                total,
                reused,
            });

            if let Some((package, assets)) = cached
                .get(&path)
                .filter(|(package, _)| package.matches(&path))
            {
                reused += 1;
                push_package(&mut index, (*package).clone(), assets.iter().cloned());
                continue;
            }
            match read_package(&path) {
                Some((package, assets)) => push_package(&mut index, package, assets),
                None => index.unreadable.push(path),
            }
        }
        progress(PackageIndexProgress {
            done: total,
            total,
            reused,
        });
        index
    }

    pub fn of_kind(&self, kind: PackageAssetKind) -> impl Iterator<Item = &PackageAsset> {
        self.assets.iter().filter(move |asset| asset.kind == kind)
    }

    pub fn counts(&self) -> BTreeMap<PackageAssetKind, usize> {
        let mut counts = BTreeMap::new();
        for asset in &self.assets {
            *counts.entry(asset.kind).or_default() += 1;
        }
        counts
    }

    pub fn package_of(&self, asset: &PackageAsset) -> Option<&PackageRef> {
        self.packages.get(asset.package as usize)
    }

    pub fn read_cache(path: &Path) -> Option<Self> {
        let bytes = fs::read(path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn write_cache(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let bytes = serde_json::to_vec(self).map_err(|error| error.to_string())?;

        let staging = path.with_extension("tmp");
        fs::write(&staging, &bytes).map_err(|error| error.to_string())?;
        fs::rename(&staging, path).map_err(|error| error.to_string())
    }
}

fn push_package(
    index: &mut PackageIndex,
    package: PackageRef,
    assets: impl IntoIterator<Item = PackageAsset>,
) {
    let Ok(ordinal) = u32::try_from(index.packages.len()) else {
        return;
    };
    index.packages.push(package);
    index.assets.extend(assets.into_iter().map(|mut asset| {
        asset.package = ordinal;
        asset
    }));
}

fn index_by_path(index: &PackageIndex) -> BTreeMap<PathBuf, (&PackageRef, Vec<PackageAsset>)> {
    let mut grouped: BTreeMap<PathBuf, (&PackageRef, Vec<PackageAsset>)> = BTreeMap::new();
    for package in &index.packages {
        grouped.insert(package.path.clone(), (package, Vec::new()));
    }
    for asset in &index.assets {
        let Some(package) = index.packages.get(asset.package as usize) else {
            continue;
        };
        if let Some((_, assets)) = grouped.get_mut(&package.path) {
            assets.push(asset.clone());
        }
    }
    grouped
}

fn archive_paths(addon_packages: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(addon_packages) else {
        return Vec::new();
    };
    entries
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("var"))
        })
        .collect()
}

fn stamp(path: &Path) -> Option<(u64, u64)> {
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis();
    Some((metadata.len(), u64::try_from(modified).unwrap_or(u64::MAX)))
}

fn read_package(path: &Path) -> Option<(PackageRef, Vec<PackageAsset>)> {
    let (len, modified_ms) = stamp(path)?;
    let entries = list_var_entries(path).ok()?;
    let package = PackageRef {
        path: path.to_path_buf(),
        uid: path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_owned(),
        len,
        modified_ms,
    };
    let assets = entries
        .into_iter()
        .filter_map(|entry| {
            classify(&entry).map(|kind| PackageAsset {
                package: 0,
                entry,
                kind,
            })
        })
        .collect();
    Some((package, assets))
}

fn classify(entry: &str) -> Option<PackageAssetKind> {
    let lowered = entry.to_ascii_lowercase();
    if lowered.ends_with('/') {
        return None;
    }
    let extension = lowered.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
    let under = |prefix: &str| lowered.starts_with(prefix);

    if under("custom/atom/person/pose/") || under("saves/person/pose/") {
        return matches!(extension, "vap" | "json").then_some(PackageAssetKind::Pose);
    }
    if under("custom/atom/person/appearance/") || under("saves/person/appearance/") {
        return matches!(extension, "vap" | "json").then_some(PackageAssetKind::Appearance);
    }
    if under("custom/atom/person/morphs/") {
        return matches!(extension, "vmi" | "vmb").then_some(PackageAssetKind::Morph);
    }
    if under("custom/atom/person/skin/") {
        return Some(PackageAssetKind::Skin);
    }
    if under("custom/atom/person/textures/") {
        return Some(PackageAssetKind::Texture);
    }
    if under("custom/atom/person/hair/") || under("custom/hair/") {
        return Some(PackageAssetKind::Hair);
    }
    if under("custom/atom/person/clothing/") || under("custom/clothing/") {
        return Some(PackageAssetKind::Clothing);
    }
    if under("saves/scene/") {
        return (extension == "json").then_some(PackageAssetKind::Scene);
    }

    if under("custom/atom/person/") && extension == "vap" {
        return Some(PackageAssetKind::OtherPreset);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pose_is_a_pose_in_either_of_the_two_places_packages_put_one() {
        assert_eq!(
            classify("Custom/Atom/Person/Pose/Creator/Standing.vap"),
            Some(PackageAssetKind::Pose)
        );
        assert_eq!(
            classify("Saves/Person/pose/Creator/Standing.json"),
            Some(PackageAssetKind::Pose)
        );

        assert_eq!(
            classify("CUSTOM/ATOM/PERSON/POSE/Creator/Standing.VAP"),
            Some(PackageAssetKind::Pose)
        );
    }

    #[test]
    fn the_picture_beside_a_preset_is_not_the_preset() {
        assert_eq!(
            classify("Custom/Atom/Person/Pose/Creator/Standing.jpg"),
            None
        );
        assert_eq!(classify("Saves/Person/pose/Creator/Standing.png"), None);
        assert_eq!(
            classify("Custom/Atom/Person/Appearance/Creator/Look.tif"),
            None
        );
    }

    #[test]
    fn both_halves_of_a_morph_are_indexed() {
        assert_eq!(
            classify("Custom/Atom/Person/Morphs/female/Creator/Nose.vmi"),
            Some(PackageAssetKind::Morph)
        );
        assert_eq!(
            classify("Custom/Atom/Person/Morphs/female/Creator/Nose.vmb"),
            Some(PackageAssetKind::Morph)
        );
    }

    #[test]
    fn the_index_does_not_collect_what_it_cannot_use() {
        assert_eq!(classify("Custom/Scripts/Creator/Plugin.cs"), None);
        assert_eq!(classify("Custom/Assets/Creator/thing.assetbundle"), None);
        assert_eq!(classify("meta.json"), None);
        assert_eq!(classify("Custom/Atom/Person/Pose/"), None);
    }

    #[test]
    fn an_asset_is_named_by_its_file_and_not_its_folder() {
        let asset = PackageAsset {
            package: 0,
            entry: "Custom/Atom/Person/Pose/Creator/Standing Pose 001.vap".to_owned(),
            kind: PackageAssetKind::Pose,
        };
        assert_eq!(asset.display_name(), "Standing Pose 001");

        let dotted = PackageAsset {
            entry: "Custom/Atom/Person/Pose/A.B.C.vap".to_owned(),
            ..asset
        };
        assert_eq!(dotted.display_name(), "A.B.C");
    }

    #[test]
    #[ignore = "reads the user's VaM install"]
    fn the_real_library_indexes_and_the_cache_pays_for_itself() {
        let Some(dir) = std::env::var_os("VKIT_ADDON_PACKAGES").map(PathBuf::from) else {
            eprintln!("set VKIT_ADDON_PACKAGES to run this");
            return;
        };
        let started = std::time::Instant::now();
        let index = PackageIndex::build(&dir, None, |_| {});
        let cold = started.elapsed();
        println!(
            "{} packages, {} indexed assets, {} unreadable, {:.2}s",
            index.packages.len(),
            index.assets.len(),
            index.unreadable.len(),
            cold.as_secs_f64()
        );
        for (kind, count) in index.counts() {
            println!("  {count:>7}  {kind:?}");
        }
        assert!(!index.packages.is_empty(), "the library must not be empty");

        let started = std::time::Instant::now();
        let mut reused_at_end = 0;
        let again = PackageIndex::build(&dir, Some(&index), |progress| {
            reused_at_end = progress.reused;
        });
        println!(
            "reindex {:.2}s, {reused_at_end} reused",
            started.elapsed().as_secs_f64()
        );
        assert_eq!(
            reused_at_end,
            index.packages.len(),
            "every package should reuse"
        );
        assert_eq!(
            again.assets, index.assets,
            "the cache must return the same library"
        );

        let cache = std::env::temp_dir().join("vkit-package-index-test.json");
        index.write_cache(&cache).expect("cache writes");
        let read = PackageIndex::read_cache(&cache).expect("cache reads");
        assert_eq!(
            read.assets, index.assets,
            "the cache must survive a round trip"
        );
        println!(
            "cache {} bytes",
            std::fs::metadata(&cache).map(|m| m.len()).unwrap_or(0)
        );
        let _ = std::fs::remove_file(&cache);
    }

    #[test]
    fn a_library_that_is_not_there_indexes_to_nothing() {
        let index = PackageIndex::build(Path::new("no/such/folder"), None, |_| {});
        assert!(index.packages.is_empty());
        assert!(index.assets.is_empty());
        assert!(index.unreadable.is_empty());
    }
}
