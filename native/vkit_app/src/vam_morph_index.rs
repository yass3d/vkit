use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use vkit_core::vam::{SkinSex, list_var_entries};

use crate::vam_edit_sources::{morph_identity, morph_identity_candidates};

const MAX_INDEXED_MORPHS: usize = 200_000;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum MorphLocation {
    Loose(PathBuf),

    Packed { archive: PathBuf, vmi_entry: String },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IndexedMorph {
    pub uid: String,

    pub name: String,
    pub sex: SkinSex,
    pub location: MorphLocation,
}

#[derive(Clone, Debug, Default)]
pub struct VaMMorphIndex {
    morphs: Vec<IndexedMorph>,
    by_identity: HashMap<String, Vec<usize>>,
    archives_scanned: usize,
}

impl VaMMorphIndex {
    pub fn build(vam_root: &Path) -> Self {
        let mut index = Self::default();
        for sex in [SkinSex::Female, SkinSex::Male] {
            let folder = morph_folder(vam_root, sex);
            index.scan_loose(&folder, sex, vam_root);
        }
        index.scan_packages(&vam_root.join("AddonPackages"));
        index
    }

    pub fn resolve(&self, reference: &str, sex: SkinSex) -> Option<&IndexedMorph> {
        let mut best: Option<usize> = None;
        for identity in morph_identity_candidates(reference) {
            let Some(slots) = self.by_identity.get(&identity) else {
                continue;
            };
            for &slot in slots {
                let better = best.is_none_or(|current| {
                    candidate_rank(&self.morphs[slot], sex)
                        > candidate_rank(&self.morphs[current], sex)
                });
                if better {
                    best = Some(slot);
                }
            }
        }
        best.map(|slot| &self.morphs[slot])
    }

    pub fn len(&self) -> usize {
        self.morphs.len()
    }

    pub fn archives_scanned(&self) -> usize {
        self.archives_scanned
    }

    fn push(&mut self, morph: IndexedMorph) {
        if self.morphs.len() >= MAX_INDEXED_MORPHS {
            return;
        }
        let slot = self.morphs.len();
        let mut identities = morph_identity_candidates(&morph.uid);
        let name_identity = morph_identity(&morph.name);
        if !name_identity.is_empty() {
            identities.push(name_identity);
        }
        identities.sort();
        identities.dedup();
        self.morphs.push(morph);

        for identity in identities {
            if !identity.is_empty() {
                self.by_identity.entry(identity).or_default().push(slot);
            }
        }
    }

    fn scan_loose(&mut self, folder: &Path, sex: SkinSex, vam_root: &Path) {
        if !folder.is_dir() {
            return;
        }
        let mut pending = vec![folder.to_path_buf()];
        while let Some(dir) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                if !has_extension(&path, "vmi") || !path.with_extension("vmb").is_file() {
                    continue;
                }
                let uid = relative_uid(vam_root, &path);
                let name = file_stem(&path);
                self.push(IndexedMorph {
                    uid,
                    name,
                    sex,
                    location: MorphLocation::Loose(path),
                });
                if self.morphs.len() >= MAX_INDEXED_MORPHS {
                    return;
                }
            }
        }
    }

    fn scan_packages(&mut self, addon_packages: &Path) {
        if !addon_packages.is_dir() {
            return;
        }
        let mut pending = vec![addon_packages.to_path_buf()];
        while let Some(dir) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                if !has_extension(&path, "var") {
                    continue;
                }

                let Ok(names) = list_var_entries(&path) else {
                    continue;
                };
                self.archives_scanned += 1;
                for morph in packed_morphs_from_entries(&path, &names) {
                    self.push(morph);
                    if self.morphs.len() >= MAX_INDEXED_MORPHS {
                        return;
                    }
                }
            }
        }
    }
}

fn packed_morphs_from_entries(archive: &Path, names: &[String]) -> Vec<IndexedMorph> {
    let package = archive
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    let normalized: HashSet<String> = names
        .iter()
        .map(|name| name.replace('\\', "/").to_ascii_lowercase())
        .collect();
    let mut morphs = Vec::new();
    for raw in names {
        let entry = raw.replace('\\', "/");
        let lower = entry.to_ascii_lowercase();
        if !lower.ends_with(".vmi") || !lower.contains("/morphs/") {
            continue;
        }

        let vmb = format!("{}.vmb", &lower[..lower.len() - 4]);
        if !normalized.contains(&vmb) {
            continue;
        }
        morphs.push(IndexedMorph {
            uid: format!("{package}:/{entry}"),
            name: Path::new(&entry)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("Morph")
                .to_owned(),
            sex: sex_from_path(&lower),
            location: MorphLocation::Packed {
                archive: archive.to_path_buf(),
                vmi_entry: entry,
            },
        });
    }
    morphs
}

fn morph_folder(vam_root: &Path, sex: SkinSex) -> PathBuf {
    let leaf = match sex {
        SkinSex::Male => "male",
        _ => "female",
    };
    vam_root
        .join("Custom")
        .join("Atom")
        .join("Person")
        .join("Morphs")
        .join(leaf)
}

fn sex_from_path(lower: &str) -> SkinSex {
    if lower.contains("/female/") {
        SkinSex::Female
    } else if lower.contains("/male/") {
        SkinSex::Male
    } else {
        SkinSex::Unknown
    }
}

fn relative_uid(vam_root: &Path, path: &Path) -> String {
    path.strip_prefix(vam_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Morph")
        .to_owned()
}

fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
}

fn candidate_rank(morph: &IndexedMorph, sex: SkinSex) -> u8 {
    let sex_match = morph.sex == sex || morph.sex == SkinSex::Unknown;
    let loose = matches!(morph.location, MorphLocation::Loose(_));
    (u8::from(sex_match) << 1) | u8::from(loose)
}

const MORPH_INDEX_CACHE_VERSION: u32 = 1;

#[derive(Deserialize, Serialize)]
struct MorphIndexCache {
    version: u32,
    fingerprint: String,
    archives_scanned: usize,
    morphs: Vec<IndexedMorph>,
}

impl VaMMorphIndex {
    pub fn load_or_build(vam_root: &Path, force_rebuild: bool) -> Self {
        let fingerprint = source_fingerprint(vam_root);
        if !force_rebuild && let Some(cached) = load_cache(vam_root, &fingerprint) {
            return cached;
        }
        let index = Self::build(vam_root);
        store_cache(vam_root, &fingerprint, &index);
        index
    }

    fn from_cache(cache: MorphIndexCache) -> Self {
        let mut index = Self {
            morphs: Vec::with_capacity(cache.morphs.len()),
            by_identity: HashMap::new(),
            archives_scanned: cache.archives_scanned,
        };
        for morph in cache.morphs {
            index.push(morph);
        }
        index
    }

    fn to_cache(&self, fingerprint: String) -> MorphIndexCache {
        MorphIndexCache {
            version: MORPH_INDEX_CACHE_VERSION,
            fingerprint,
            archives_scanned: self.archives_scanned,
            morphs: self.morphs.clone(),
        }
    }
}

fn source_fingerprint(vam_root: &Path) -> String {
    fn visit(directory: &Path, extension: &str, entries: &mut Vec<String>, depth: usize) {
        if depth > 8 {
            return;
        }
        let Ok(listing) = std::fs::read_dir(directory) else {
            return;
        };
        for entry in listing.flatten() {
            let path = entry.path();
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                visit(&path, extension, entries, depth + 1);
                continue;
            }
            if !has_extension(&path, extension) {
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

    let mut entries = Vec::new();
    for sex in [SkinSex::Female, SkinSex::Male] {
        visit(&morph_folder(vam_root, sex), "vmi", &mut entries, 0);
    }
    visit(&vam_root.join("AddonPackages"), "var", &mut entries, 0);
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

fn cache_path(vam_root: &Path) -> Option<PathBuf> {
    let key = crate::cache_paths::cache_key_for_root(vam_root);
    Some(
        vkit_core::cache_root()?
            .join("morph-index")
            .join(format!("index-{key}.json")),
    )
}

fn load_cache(vam_root: &Path, fingerprint: &str) -> Option<VaMMorphIndex> {
    let path = cache_path(vam_root)?;
    let bytes = std::fs::read(path).ok()?;
    let cache: MorphIndexCache = serde_json::from_slice(&bytes).ok()?;
    (cache.version == MORPH_INDEX_CACHE_VERSION && cache.fingerprint == fingerprint)
        .then(|| VaMMorphIndex::from_cache(cache))
}

fn store_cache(vam_root: &Path, fingerprint: &str, index: &VaMMorphIndex) {
    let Some(path) = cache_path(vam_root) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec(&index.to_cache(fingerprint.to_owned())) {
        let _ = std::fs::write(path, bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "vkit-morph-index-{label}-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn packed_entries_pair_vmi_with_vmb_and_skip_orphans_and_non_morphs() {
        let names = vec![
            "Custom/Atom/Person/Morphs/female/Head/FaceShape.vmi".to_owned(),
            "Custom/Atom/Person/Morphs/female/Head/FaceShape.vmb".to_owned(),
            "Custom/Atom/Person/Morphs/female/Head/NoBinary.vmi".to_owned(),
            "Custom/Atom/Person/Textures/skin.png".to_owned(),
            "Custom/Atom/Person/Morphs/male/Jaw.VMI".to_owned(),
            "Custom/Atom/Person/Morphs/male/Jaw.VMB".to_owned(),
        ];
        let morphs = packed_morphs_from_entries(Path::new("Creator.Shapes.3.var"), &names);
        assert_eq!(morphs.len(), 2, "only complete morph pairs are indexed");

        let face = morphs.iter().find(|m| m.name == "FaceShape").unwrap();
        assert_eq!(face.sex, SkinSex::Female);
        assert_eq!(
            face.uid,
            "Creator.Shapes.3:/Custom/Atom/Person/Morphs/female/Head/FaceShape.vmi"
        );
        let jaw = morphs.iter().find(|m| m.name == "Jaw").unwrap();
        assert_eq!(jaw.sex, SkinSex::Male, "case-different pair still resolves");
    }

    #[test]
    fn loose_pairs_are_indexed_and_resolve_by_bare_name_and_path() {
        let root = temp_root("loose");
        let folder = morph_folder(&root, SkinSex::Female).join("Author");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("Cheekbones.vmi"), b"{}").unwrap();
        std::fs::write(folder.join("Cheekbones.vmb"), b"x").unwrap();

        std::fs::write(folder.join("Loner.vmi"), b"{}").unwrap();

        let index = VaMMorphIndex::build(&root);
        assert_eq!(index.len(), 1);

        let by_name = index
            .resolve("Cheekbones", SkinSex::Female)
            .expect("resolves by bare name");
        assert_eq!(by_name.sex, SkinSex::Female);
        assert!(matches!(by_name.location, MorphLocation::Loose(_)));

        assert!(
            index
                .resolve(
                    "Custom/Atom/Person/Morphs/female/Author/Cheekbones.vmi",
                    SkinSex::Female
                )
                .is_some()
        );
        assert!(index.resolve("NothingHere", SkinSex::Female).is_none());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_loose_morph_overrides_a_packaged_one_of_the_same_identity() {
        let mut index = VaMMorphIndex::default();
        index.push(IndexedMorph {
            uid: "Creator.Pack.1:/Custom/Atom/Person/Morphs/female/Nose.vmi".to_owned(),
            name: "Nose".to_owned(),
            sex: SkinSex::Female,
            location: MorphLocation::Packed {
                archive: PathBuf::from("Creator.Pack.1.var"),
                vmi_entry: "Custom/Atom/Person/Morphs/female/Nose.vmi".to_owned(),
            },
        });
        index.push(IndexedMorph {
            uid: "Custom/Atom/Person/Morphs/female/Nose.vmi".to_owned(),
            name: "Nose".to_owned(),
            sex: SkinSex::Female,
            location: MorphLocation::Loose(PathBuf::from("/vam/Custom/.../Nose.vmi")),
        });
        assert!(
            matches!(
                index.resolve("Nose", SkinSex::Female).unwrap().location,
                MorphLocation::Loose(_)
            ),
            "the user's loose morph wins over the packaged one"
        );
    }

    #[test]
    fn cache_round_trip_rebuilds_the_index_and_its_resolution() {
        let mut index = VaMMorphIndex {
            archives_scanned: 3,
            ..Default::default()
        };
        index.push(IndexedMorph {
            uid: "Creator.Pack.2:/Custom/Atom/Person/Morphs/female/Nose.vmi".to_owned(),
            name: "Nose".to_owned(),
            sex: SkinSex::Female,
            location: MorphLocation::Packed {
                archive: PathBuf::from("Creator.Pack.2.var"),
                vmi_entry: "Custom/Atom/Person/Morphs/female/Nose.vmi".to_owned(),
            },
        });
        let bytes = serde_json::to_vec(&index.to_cache("fingerprint".to_owned())).unwrap();
        let cache: MorphIndexCache = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(cache.version, MORPH_INDEX_CACHE_VERSION);

        let restored = VaMMorphIndex::from_cache(cache);
        assert_eq!(restored.len(), 1);
        assert_eq!(restored.archives_scanned(), 3);

        assert!(restored.resolve("Nose", SkinSex::Female).is_some());
    }

    #[test]
    fn fingerprint_changes_when_a_morph_is_added() {
        let root = temp_root("fingerprint");
        let folder = morph_folder(&root, SkinSex::Female);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("A.vmi"), b"{}").unwrap();
        let before = source_fingerprint(&root);
        std::fs::write(folder.join("B.vmi"), b"{}").unwrap();
        let after = source_fingerprint(&root);
        assert_ne!(before, after, "adding a morph must invalidate the cache");
        let _ = std::fs::remove_dir_all(&root);
    }
}
