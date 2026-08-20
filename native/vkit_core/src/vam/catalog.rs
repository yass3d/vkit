use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::{Result, VaMError, io_error};
use super::{
    geometry::GeometrySex,
    skin::SkinSex,
    unity_morph_bank::{BuiltinMorphSession, Formula, RawMorphMetadata},
};

pub const EYELID_LOOK_CONTROLS: &[&str] = &[
    "PHMEyelidsTopDownL",
    "PHMEyelidsTopDownR",
    "PHMEyeLidsTopUpL",
    "PHMEyeLidsTopUpR",
    "PHMEyeLidsBottomDownL",
    "PHMEyeLidsBottomDownR",
    "PHMEyeLidsBottomUpL",
    "PHMEyeLidsBottomUpR",
    "CTRLEyeLidsTopDown",
    "CTRLEyeLidsTopUp",
    "CTRLEyeLidsBottomDown",
    "CTRLEyeLidsBottomUp",
];

#[must_use]
pub fn is_eyelid_look_control(internal_name: &str) -> bool {
    EYELID_LOOK_CONTROLS
        .iter()
        .any(|control| control.eq_ignore_ascii_case(internal_name))
}

const HIDDEN_EYE_CONTROLS: &[&str] = &[
    "CTRLEyesUpDown",
    "CTRLEyesSide-Side",
    "CTRLEyeLidsTopDown",
    "CTRLEyeLidsBottomDown",
    "CTRLEyeLidsTopUp",
    "CTRLEyeLidsBottomUp",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaMRoot {
    path: PathBuf,
    female_bank: PathBuf,
    male_bank: PathBuf,
}

impl VaMRoot {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let requested = path.as_ref();
        let metadata = fs::metadata(requested).map_err(|error| io_error(requested, error))?;
        if !metadata.is_dir() {
            return Err(VaMError::InvalidRoot {
                path: requested.to_path_buf(),
                message: "selected path is not a directory".to_owned(),
            });
        }
        let canonical = fs::canonicalize(requested).map_err(|error| io_error(requested, error))?;
        let female_bank = canonical
            .join("VaM_Data")
            .join("StreamingAssets")
            .join("f_mb");
        let male_bank = canonical
            .join("VaM_Data")
            .join("StreamingAssets")
            .join("m_mb");

        let has_recognized_layout = canonical.join("VaM_Data").is_dir()
            || canonical.join("AddonPackages").is_dir()
            || canonical
                .join("Custom")
                .join("Atom")
                .join("Person")
                .is_dir();
        if !has_recognized_layout {
            return Err(VaMError::InvalidRoot {
                path: canonical.clone(),
                message: "no VaM_Data, AddonPackages, or Custom/Atom/Person assets were found"
                    .to_owned(),
            });
        }
        Ok(Self {
            path: canonical,
            female_bank,
            male_bank,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn female_bank_path(&self) -> &Path {
        &self.female_bank
    }

    pub fn morph_bank_path(&self, sex: SkinSex) -> Result<&Path> {
        let (path, relative_path) = match sex {
            SkinSex::Female => (self.female_bank_path(), "VaM_Data/StreamingAssets/f_mb"),
            SkinSex::Male => (self.male_bank.as_path(), "VaM_Data/StreamingAssets/m_mb"),
            SkinSex::Unknown => Err(VaMError::InvalidRoot {
                path: self.path.clone(),
                message: "figure sex must be Female or Male".to_owned(),
            })?,
        };
        if path.is_file() {
            Ok(path)
        } else {
            Err(VaMError::InvalidRoot {
                path: self.path.clone(),
                message: format!("{relative_path} was not found"),
            })
        }
    }

    pub fn addon_packages_path(&self) -> PathBuf {
        self.path.join("AddonPackages")
    }

    pub fn person_assets_path(&self) -> PathBuf {
        self.path.join("Custom").join("Atom").join("Person")
    }

    pub fn person_atom_bundle_path(&self) -> PathBuf {
        self.path
            .join("VaM_Data")
            .join("StreamingAssets")
            .join("a_per")
    }

    pub fn neutral_base_bundle_path(&self, sex: GeometrySex) -> PathBuf {
        self.path
            .join("VaM_Data")
            .join("StreamingAssets")
            .join(match sex {
                GeometrySex::Female => "f_1",
                GeometrySex::Male => "m_1",
            })
    }

    pub fn geometry_base_candidates(&self, sex: GeometrySex) -> Vec<PathBuf> {
        geometry_base_candidate_names(sex)
            .iter()
            .flat_map(|name| {
                [
                    self.path.join(name),
                    self.person_assets_path().join("Geometry").join(name),
                ]
            })
            .filter(|candidate| candidate.is_file())
            .collect()
    }

    pub fn morph_output_directory(&self, sex: GeometrySex) -> PathBuf {
        self.person_assets_path().join("Morphs").join(match sex {
            GeometrySex::Female => "female",
            GeometrySex::Male => "male",
        })
    }
}

pub fn geometry_base_candidate_names(sex: GeometrySex) -> &'static [&'static str] {
    match sex {
        GeometrySex::Female => &[
            "femalecustom.obj",
            "female_custom.obj",
            "femalecustom_skinned.obj",
            "female1-BaseF.obj",
        ],
        GeometrySex::Male => &[
            "malecustom.obj",
            "malecustom_skinned.obj",
            "male2-Hector.obj",
            "male2-Hector_skinned.obj",
            "male1-Michael.obj",
            "male1-Michael_skinned.obj",
            "male3-Darius.obj",
            "male3-Darius_skinned.obj",
        ],
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MorphCategory {
    Eyes,
    Brows,
    Nose,
    Mouth,
    Jaw,
    Cheeks,
    Ears,
    Expression,
    Head,

    Cheekbones,

    Body,
}

impl MorphCategory {
    #[must_use]
    pub const fn describes_resting_shape(self) -> bool {
        !matches!(self, Self::Expression)
    }
}

const POSE_RIG_SEGMENT: &str = "posecontrols";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BuiltinMorphSource {
    pub bundle_path: PathBuf,
    pub object_path_id: i64,
    pub morph_index: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuiltinMorphDescriptor {
    pub stable_id: String,
    pub internal_name: String,
    pub label: String,
    pub region: String,
    pub group: String,
    pub category: MorphCategory,
    pub visible: bool,
    pub diagnostic_hidden: bool,
    pub disabled: bool,
    pub is_pose_control: bool,
    pub minimum: f64,
    pub maximum: f64,
    pub default: f64,
    pub delta_count: usize,
    pub formulas: Vec<Formula>,
    pub source: BuiltinMorphSource,
}

impl BuiltinMorphDescriptor {
    #[must_use]
    pub fn is_offered_control(&self) -> bool {
        if is_eyelid_look_control(&self.internal_name) {
            return !self.disabled;
        }
        self.visible
            && !self.diagnostic_hidden
            && !self.disabled
            && (!self.is_pose_control || self.category == MorphCategory::Expression)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MorphCatalog {
    entries: Vec<BuiltinMorphDescriptor>,
}

impl MorphCatalog {
    pub fn scan_female_builtin(root: &VaMRoot) -> Result<Self> {
        let session = BuiltinMorphSession::open(root)?;
        Ok(Self::from_session(&session))
    }

    pub fn from_session(session: &BuiltinMorphSession) -> Self {
        Self::from_records(session.metadata().to_vec(), session.bundle_path())
    }

    pub fn is_person_specific_morph(
        internal_name: &str,
        label: &str,
        region: &str,
        group: &str,
    ) -> bool {
        is_person_specific_morph(internal_name, label, region, group)
    }

    pub fn refine_category(
        category: MorphCategory,
        internal_name: &str,
        label: &str,
    ) -> MorphCategory {
        if has_expression_semantics([internal_name, label]) {
            MorphCategory::Expression
        } else if has_cheekbone_semantics([internal_name, label]) {
            MorphCategory::Cheekbones
        } else {
            category
        }
    }

    fn from_records(raw: Vec<RawMorphMetadata>, bundle_path: &Path) -> Self {
        let is_hidden = |name: &str| {
            HIDDEN_EYE_CONTROLS
                .iter()
                .any(|control| control.eq_ignore_ascii_case(name))
        };
        let mut entries = Vec::new();
        for record in raw {
            if Self::is_person_specific_morph(
                &record.internal_name,
                &record.label,
                &record.region,
                &record.group,
            ) {
                continue;
            }

            if normalized_terms(&record.internal_name).join("") == "eyesclosed"
                || normalized_terms(&record.label).join("") == "eyesclosed"
            {
                continue;
            }
            let explicitly_hidden = is_hidden(&record.internal_name);
            let category = if explicitly_hidden {
                Some(MorphCategory::Eyes)
            } else {
                classify_person_morph(
                    &record.region,
                    &record.group,
                    &record.internal_name,
                    &record.label,
                )
            };
            let Some(category) = category else {
                continue;
            };
            let normal_visibility = record.visible && !record.disabled;
            if !normal_visibility && !explicitly_hidden {
                continue;
            }
            let source = BuiltinMorphSource {
                bundle_path: bundle_path.to_path_buf(),
                object_path_id: record.object_path_id,
                morph_index: record.morph_index,
            };
            entries.push(BuiltinMorphDescriptor {
                stable_id: stable_id(&record.internal_name, &source),
                internal_name: record.internal_name,
                label: record.label,
                region: record.region,
                group: record.group,
                category,
                visible: record.visible,
                diagnostic_hidden: !normal_visibility && explicitly_hidden,
                disabled: record.disabled,
                is_pose_control: record.is_pose_control,
                minimum: record.minimum,
                maximum: record.maximum,
                default: record.default,
                delta_count: record.delta_count,
                formulas: record.formulas,
                source,
            });
        }
        entries.sort_by(|left, right| {
            (
                left.category,
                left.label.to_lowercase(),
                left.internal_name.to_lowercase(),
                &left.source,
            )
                .cmp(&(
                    right.category,
                    right.label.to_lowercase(),
                    right.internal_name.to_lowercase(),
                    &right.source,
                ))
        });
        entries.dedup_by(|left, right| left.source == right.source);
        Self { entries }
    }

    pub fn entries(&self) -> &[BuiltinMorphDescriptor] {
        &self.entries
    }

    pub fn query(
        &self,
        category: Option<MorphCategory>,
        search: &str,
    ) -> Vec<&BuiltinMorphDescriptor> {
        self.query_with_options(category, search, false, false)
    }

    pub fn query_with_options(
        &self,
        category: Option<MorphCategory>,
        search: &str,
        include_diagnostic_hidden: bool,
        include_pose_controls: bool,
    ) -> Vec<&BuiltinMorphDescriptor> {
        let terms = normalized_terms(search);
        self.entries
            .iter()
            .filter(|entry| category.is_none_or(|wanted| entry.category == wanted))
            .filter(|entry| include_diagnostic_hidden || !entry.diagnostic_hidden)
            .filter(|entry| {
                include_pose_controls
                    || !entry.is_pose_control
                    || entry.category == MorphCategory::Expression
                    || is_eyelid_look_control(&entry.internal_name)
            })
            .filter(|entry| {
                if terms.is_empty() {
                    return true;
                }
                let haystack = normalized_terms(&format!(
                    "{} {} {} {}",
                    entry.label, entry.internal_name, entry.region, entry.group
                ))
                .join(" ");
                terms.iter().all(|term| haystack.contains(term))
            })
            .collect()
    }
}

fn stable_id(name: &str, source: &BuiltinMorphSource) -> String {
    let slug = normalized_terms(name).join("-");
    format!(
        "vam:builtin:f:{:016x}:{}:{slug}",
        source.object_path_id as u64, source.morph_index
    )
}

fn classify_person_morph(
    region: &str,
    group: &str,
    internal_name: &str,
    label: &str,
) -> Option<MorphCategory> {
    let segments: BTreeSet<String> = [region, group]
        .into_iter()
        .flat_map(|value| value.split(['/', '\\']))
        .map(normalized_segment)
        .filter(|value| !value.is_empty())
        .collect();

    let has = |candidates: &[&str]| candidates.iter().any(|value| segments.contains(*value));
    if has_expression_semantics([region, group, internal_name, label]) {
        Some(MorphCategory::Expression)
    } else if has_cheekbone_semantics([region, group, internal_name, label]) {
        Some(MorphCategory::Cheekbones)
    } else if has(&[POSE_RIG_SEGMENT]) && has(&["tongue"]) {
        Some(MorphCategory::Expression)
    } else if has(&["brows", "brow", "eyebrows", "eyebrow"]) {
        Some(MorphCategory::Brows)
    } else if has(&["eyes", "eye", "eyelids", "eyelid"]) {
        Some(MorphCategory::Eyes)
    } else if has(&["nose", "nostrils", "nostril"]) {
        Some(MorphCategory::Nose)
    } else if has(&["mouth", "lips", "lip", "visemes", "viseme"]) {
        Some(MorphCategory::Mouth)
    } else if has(&["jaw", "chin"]) {
        Some(MorphCategory::Jaw)
    } else if has(&["cheeks", "cheek"]) {
        Some(MorphCategory::Cheeks)
    } else if has(&["ears", "ear"]) {
        Some(MorphCategory::Ears)
    } else if has(&["expressions", "expression"]) {
        Some(MorphCategory::Expression)
    } else if has(&["face", "head"]) {
        Some(MorphCategory::Head)
    } else if has(BODY_TERMS) {
        Some(MorphCategory::Body)
    } else {
        None
    }
}

const BODY_TERMS: &[&str] = &[
    "body",
    "torso",
    "chest",
    "breast",
    "breasts",
    "nipple",
    "nipples",
    "pectoral",
    "waist",
    "stomach",
    "belly",
    "abdomen",
    "navel",
    "hip",
    "hips",
    "pelvis",
    "glute",
    "glutes",
    "buttock",
    "buttocks",
    "butt",
    "back",
    "shoulder",
    "shoulders",
    "collar",
    "arm",
    "arms",
    "forearm",
    "elbow",
    "wrist",
    "hand",
    "hands",
    "finger",
    "fingers",
    "thumb",
    "leg",
    "legs",
    "thigh",
    "thighs",
    "knee",
    "calf",
    "calves",
    "shin",
    "ankle",
    "foot",
    "feet",
    "toe",
    "toes",
    "height",
    "muscle",
    "muscles",
    "weight",
    "figure",
    "physique",
    "proportions",
];

const KNOWN_PERSON_NAMES: &[&str] = &[
    "adriana",
    "adrianna",
    "aiko",
    "alessa",
    "ana",
    "aneta",
    "beau",
    "bo",
    "candy",
    "carmen",
    "daisy",
    "danika",
    "darius",
    "destiny",
    "eisa",
    "gia",
    "gianni",
    "greta",
    "hector",
    "izarra",
    "jennifer",
    "josie",
    "julian",
    "kimi",
    "kori",
    "lee",
    "lexi",
    "lilith",
    "lin",
    "lorraine",
    "lucille",
    "lyric",
    "mack",
    "madeline",
    "maria",
    "mei",
    "mia",
    "michael",
    "monique",
    "neomi",
    "norma",
    "nyssa",
    "olympia",
    "parisa",
    "ryze",
    "scott",
    "stephanie",
    "sumiko",
    "tara",
    "taric",
    "vianne",
    "victoria",
];

fn is_person_specific_morph(internal_name: &str, label: &str, region: &str, group: &str) -> bool {
    let internal_tokens = metadata_tokens(internal_name);
    let label_tokens = metadata_tokens(label);
    let path_tokens = [region, group]
        .into_iter()
        .flat_map(metadata_tokens)
        .collect::<Vec<_>>();
    let all_tokens = internal_tokens
        .iter()
        .chain(&label_tokens)
        .chain(&path_tokens);

    if all_tokens
        .clone()
        .any(|token| KNOWN_PERSON_NAMES.contains(&token.as_str()))
    {
        return true;
    }

    if path_tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "actor"
                | "actors"
                | "character"
                | "characters"
                | "identities"
                | "identity"
                | "people"
                | "person"
        )
    }) {
        return true;
    }

    let has_identity_prefix = internal_tokens
        .iter()
        .any(|token| matches!(token.as_str(), "fbm" | "fhm"));
    has_identity_prefix
        && (has_person_name_shape(&internal_tokens) || has_person_name_shape(&label_tokens))
}

fn has_person_name_shape(tokens: &[String]) -> bool {
    let semantic = tokens
        .iter()
        .map(String::as_str)
        .filter(|token| {
            !matches!(*token, "ctrl" | "fbm" | "fhm" | "pbm" | "phm")
                && !token.bytes().all(|byte| byte.is_ascii_digit())
        })
        .collect::<Vec<_>>();
    semantic.windows(2).any(|pair| {
        is_person_name_candidate(pair[0])
            && matches!(pair[1], "body" | "character" | "face" | "head" | "person")
    }) || semantic
        .first()
        .is_some_and(|token| semantic.len() == 1 && is_person_name_candidate(token))
}

fn is_person_name_candidate(token: &str) -> bool {
    token.len() >= 3
        && token.bytes().all(|byte| byte.is_ascii_alphabetic())
        && !matches!(
            token,
            "adult"
                | "african"
                | "aged"
                | "angle"
                | "anime"
                | "asian"
                | "back"
                | "base"
                | "beautiful"
                | "big"
                | "body"
                | "bottom"
                | "bridge"
                | "brow"
                | "brows"
                | "center"
                | "cheek"
                | "cheekbone"
                | "cheekbones"
                | "cheeks"
                | "child"
                | "chin"
                | "close"
                | "closed"
                | "control"
                | "crease"
                | "cranium"
                | "curve"
                | "custom"
                | "default"
                | "depth"
                | "down"
                | "ear"
                | "ears"
                | "elf"
                | "eye"
                | "eyeball"
                | "eyeballs"
                | "eyebrow"
                | "eyebrows"
                | "eyelid"
                | "eyelids"
                | "eyes"
                | "face"
                | "faerie"
                | "fairy"
                | "fbm"
                | "female"
                | "feminine"
                | "fhm"
                | "fix"
                | "forward"
                | "full"
                | "generic"
                | "head"
                | "height"
                | "human"
                | "inner"
                | "jaw"
                | "large"
                | "left"
                | "length"
                | "line"
                | "lines"
                | "lip"
                | "lips"
                | "lower"
                | "male"
                | "masculine"
                | "middle"
                | "morph"
                | "mouth"
                | "move"
                | "narrow"
                | "neck"
                | "nose"
                | "old"
                | "older"
                | "open"
                | "oval"
                | "outer"
                | "pbm"
                | "phm"
                | "position"
                | "profile"
                | "realism"
                | "realistic"
                | "right"
                | "rotation"
                | "round"
                | "scale"
                | "shape"
                | "side"
                | "size"
                | "slope"
                | "small"
                | "square"
                | "stylized"
                | "teen"
                | "thin"
                | "top"
                | "tongue"
                | "upper"
                | "volume"
                | "wide"
                | "width"
                | "young"
        )
}

fn has_cheekbone_semantics<const N: usize>(values: [&str; N]) -> bool {
    let tokens = values
        .into_iter()
        .flat_map(metadata_tokens)
        .collect::<Vec<_>>();
    tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "cheekbone" | "cheekbones" | "malar" | "zygomatic"
        )
    }) || tokens.windows(2).any(|pair| {
        matches!(pair[0].as_str(), "cheek" | "cheeks")
            && matches!(pair[1].as_str(), "bone" | "bones")
    })
}

fn has_expression_semantics<const N: usize>(values: [&str; N]) -> bool {
    values.into_iter().flat_map(metadata_tokens).any(|token| {
        matches!(
            token.as_str(),
            "smile" | "smiles" | "smiling" | "grin" | "grins" | "grinning"
        )
    })
}

fn metadata_tokens(value: &str) -> Vec<String> {
    let characters = value.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut current = String::new();

    for (index, &character) in characters.iter().enumerate() {
        if !character.is_ascii_alphanumeric() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            continue;
        }

        let previous = characters.get(index.wrapping_sub(1)).copied();
        let next = characters.get(index + 1).copied();
        let boundary = !current.is_empty()
            && (previous
                .is_some_and(|previous| previous.is_ascii_digit() != character.is_ascii_digit())
                || (character.is_ascii_uppercase()
                    && previous.is_some_and(|value| value.is_ascii_lowercase())
                    || character.is_ascii_uppercase()
                        && previous.is_some_and(|value| value.is_ascii_uppercase())
                        && next.is_some_and(|value| value.is_ascii_lowercase())));
        if boundary {
            tokens.push(std::mem::take(&mut current));
        }
        current.push(character.to_ascii_lowercase());
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn normalized_segment(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn normalized_terms(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .map(str::trim)
        .filter(|term| !term.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_base_candidates_are_bounded_ordered_and_shared_with_uv() {
        let directory = std::env::temp_dir().join(format!(
            "vkit-vam-catalog-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(directory.join("VaM_Data")).unwrap();
        std::fs::write(directory.join("femalecustom.obj"), []).unwrap();
        std::fs::write(directory.join("female1-BaseF.obj"), []).unwrap();
        std::fs::write(directory.join("male2-Hector.obj"), []).unwrap();
        std::fs::write(directory.join("rentina.obj"), []).unwrap();
        let root = VaMRoot::open(&directory).unwrap();

        let female_names = root
            .geometry_base_candidates(GeometrySex::Female)
            .into_iter()
            .filter_map(|path| path.file_name().map(|name| name.to_owned()))
            .collect::<Vec<_>>();

        assert_eq!(female_names, ["femalecustom.obj", "female1-BaseF.obj"]);
        let male_names = root
            .geometry_base_candidates(GeometrySex::Male)
            .into_iter()
            .filter_map(|path| path.file_name().map(|name| name.to_owned()))
            .collect::<Vec<_>>();
        assert_eq!(male_names, ["male2-Hector.obj"]);
        assert_eq!(geometry_base_candidate_names(GeometrySex::Male).len(), 8);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn neutral_base_bundle_paths_point_into_streaming_assets() {
        let directory =
            std::env::temp_dir().join(format!("vkit-vam-bundle-path-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(directory.join("VaM_Data").join("StreamingAssets")).unwrap();
        let root = VaMRoot::open(&directory).unwrap();
        assert!(
            root.neutral_base_bundle_path(GeometrySex::Female)
                .ends_with(std::path::Path::new("VaM_Data/StreamingAssets/f_1"))
        );
        assert!(
            root.neutral_base_bundle_path(GeometrySex::Male)
                .ends_with(std::path::Path::new("VaM_Data/StreamingAssets/m_1"))
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn category_filter_uses_exact_path_segments() {
        assert_eq!(
            classify_person_morph("Pose Controls/Head/Eyes", "", "", ""),
            Some(MorphCategory::Eyes)
        );
        assert_eq!(
            classify_person_morph("Pose Controls/Head/Mouth", "", "", ""),
            Some(MorphCategory::Mouth)
        );

        assert_eq!(
            classify_person_morph("Body/63-Anus-In.Out-WideArea", "", "", ""),
            Some(MorphCategory::Body)
        );
        assert_eq!(
            classify_person_morph("Body/Forearm", "", "", ""),
            Some(MorphCategory::Body)
        );

        assert_eq!(
            classify_person_morph("Body/Chin", "", "", ""),
            Some(MorphCategory::Jaw)
        );

        assert_eq!(
            classify_person_morph("Body/Chest to Chin", "", "", ""),
            Some(MorphCategory::Body)
        );

        assert_eq!(classify_person_morph("Custom/Unnamed", "", "", ""), None);
        assert_eq!(
            classify_person_morph("Pose Controls/Head/Brows", "", "", ""),
            Some(MorphCategory::Brows)
        );
    }

    #[test]
    fn named_identity_filter_uses_exact_tokens_and_character_metadata() {
        for (internal_name, label) in [
            ("FHMAiko6Head", "Aiko 6 Head"),
            ("FHMAdrianaHead", "Adriana Head"),
            ("FHMBoHead", "Bo Head"),
            ("FHMZaraHead", "Zara Head"),
            ("SC_AlessaEars", "Alessa Ears"),
        ] {
            assert!(MorphCatalog::is_person_specific_morph(
                internal_name,
                label,
                "Head/Face",
                ""
            ));
        }
        assert!(MorphCatalog::is_person_specific_morph(
            "VendorShape",
            "Zara",
            "Head/Characters",
            ""
        ));
        for (internal_name, label) in [
            ("PHMBottomLip", "Bottom Lip"),
            ("PHMCheekBottom", "Cheek Bottom"),
            ("FHMYoungFemaleHead", "Young Female Head"),
            ("FHMHeadHeight", "Head Height"),
            ("FHMHeadWidth", "Head Width"),
            ("FHMFaceDepth", "Face Depth"),
            ("FHMHeadScale", "Head Scale"),
        ] {
            assert!(!MorphCatalog::is_person_specific_morph(
                internal_name,
                label,
                "Head/Mouth",
                ""
            ));
        }
    }

    #[test]
    fn cheekbone_compounds_are_distinct_from_soft_cheek_controls() {
        for (internal_name, label) in [
            ("PHMCheekBonesWidthUpper", "Cheek Bones Width Upper"),
            ("PHMCheekboneHeight", "Cheekbone Height"),
            ("PHMZygomaticWidth", "Zygomatic Width"),
        ] {
            assert_eq!(
                classify_person_morph("Pose Controls/Head/Cheeks", "", internal_name, label),
                Some(MorphCategory::Cheekbones)
            );
        }
        for (internal_name, label) in [
            ("PHMCheekJowl", "Cheek Jowl"),
            ("PHMCheekHollows", "Cheek Hollows"),
            ("PHMCheekVolume", "Cheek Volume"),
        ] {
            assert_eq!(
                classify_person_morph("Pose Controls/Head/Cheeks", "", internal_name, label),
                Some(MorphCategory::Cheeks)
            );
        }
    }

    #[test]
    fn smile_semantics_override_the_mouth_metadata_path() {
        for (internal_name, label) in [
            ("mouth_smile_open", "Open Mouth Smile"),
            ("PHMMouthSmileOpen", "Mouth Smile Open"),
            ("CTRLGrinning", "Grinning"),
        ] {
            assert_eq!(
                classify_person_morph("Pose Controls/Head/Mouth", "Mouth", internal_name, label),
                Some(MorphCategory::Expression)
            );
            assert_eq!(
                MorphCatalog::refine_category(MorphCategory::Mouth, internal_name, label),
                MorphCategory::Expression
            );
        }

        assert_eq!(
            classify_person_morph(
                "Pose Controls/Head/Mouth",
                "Mouth",
                "PHMMouthOpen",
                "Mouth Open"
            ),
            Some(MorphCategory::Mouth)
        );
    }

    #[test]
    fn the_pose_rig_separates_a_tongue_that_performs_from_one_that_is_shaped() {
        for (internal_name, label) in [
            ("CTRLTongueIn-Out", "Tongue In-Out"),
            ("Tongue Roll 1", "Tongue Roll 1"),
            ("CTRLTongueCurl", "Tongue Curl"),
        ] {
            assert_eq!(
                classify_person_morph(
                    "Pose Controls/Head/Mouth/Tongue",
                    "Pose Controls/Head/Mouth/Tongue",
                    internal_name,
                    label
                ),
                Some(MorphCategory::Expression)
            );
        }

        for (internal_name, label) in [
            ("Tongue Thickness", "Tongue Thickness"),
            ("CTRLTongueNarrow-Wide", "Tongue Narrow-Wide"),
        ] {
            assert_eq!(
                classify_person_morph("Morph/Head/Mouth/Tongue", "", internal_name, label),
                Some(MorphCategory::Mouth)
            );
        }
    }

    #[test]
    fn expressions_survive_the_pose_rig_exclusion_and_the_rest_of_it_does_not() {
        let descriptor = |category: MorphCategory, is_pose_control: bool| BuiltinMorphDescriptor {
            stable_id: "id".to_owned(),
            internal_name: "PHMAngry".to_owned(),
            label: "Angry".to_owned(),
            region: "Pose Controls/Head/Expressions".to_owned(),
            group: "Pose Controls/Head/Expressions".to_owned(),
            category,
            visible: true,
            diagnostic_hidden: false,
            disabled: false,
            is_pose_control,
            minimum: 0.0,
            maximum: 1.0,
            default: 0.0,
            delta_count: 1,
            formulas: Vec::new(),
            source: BuiltinMorphSource {
                bundle_path: PathBuf::from("bundle"),
                object_path_id: 1,
                morph_index: 0,
            },
        };

        assert!(descriptor(MorphCategory::Expression, true).is_offered_control());
        assert!(!descriptor(MorphCategory::Mouth, true).is_offered_control());
        assert!(descriptor(MorphCategory::Mouth, false).is_offered_control());

        let mut hidden = descriptor(MorphCategory::Expression, true);
        hidden.visible = false;
        assert!(!hidden.is_offered_control());
    }

    #[test]
    fn root_validation_rejects_an_unrecognized_directory() {
        let missing = std::env::temp_dir().join(format!("vkit-vam-missing-{}", std::process::id()));
        let _ = fs::remove_dir_all(&missing);
        fs::create_dir_all(&missing).unwrap();
        assert!(VaMRoot::open(&missing).is_err());
        fs::remove_dir_all(missing).unwrap();
    }

    #[test]
    fn male_only_root_opens_without_a_female_bank() {
        let root = std::env::temp_dir().join(format!("vkit-vam-male-only-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let banks = root.join("VaM_Data").join("StreamingAssets");
        fs::create_dir_all(&banks).unwrap();
        fs::write(banks.join("m_mb"), b"male").unwrap();

        let opened = VaMRoot::open(&root).unwrap();
        assert!(opened.morph_bank_path(SkinSex::Female).is_err());
        assert_eq!(
            opened.morph_bank_path(SkinSex::Male).unwrap(),
            fs::canonicalize(banks.join("m_mb")).unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn skin_only_root_opens_but_each_missing_bank_fails_independently() {
        let root = std::env::temp_dir().join(format!("vkit-vam-skin-only-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("Custom").join("Atom").join("Person").join("Skin")).unwrap();

        let opened = VaMRoot::open(&root).unwrap();
        let female_error = opened
            .morph_bank_path(SkinSex::Female)
            .unwrap_err()
            .to_string();
        let male_error = opened
            .morph_bank_path(SkinSex::Male)
            .unwrap_err()
            .to_string();
        assert!(female_error.contains("f_mb"));
        assert!(!female_error.contains("m_mb"));
        assert!(male_error.contains("m_mb"));
        assert!(!male_error.contains("f_mb"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn root_routes_each_explicit_figure_to_its_own_morph_bank() {
        let root = std::env::temp_dir().join(format!("vkit-vam-sex-banks-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let banks = root.join("VaM_Data").join("StreamingAssets");
        fs::create_dir_all(&banks).unwrap();
        fs::write(banks.join("f_mb"), b"female").unwrap();

        let opened = VaMRoot::open(&root).unwrap();
        let canonical_banks = fs::canonicalize(&banks).unwrap();
        assert_eq!(
            opened.morph_bank_path(SkinSex::Female).unwrap(),
            canonical_banks.join("f_mb")
        );
        assert!(opened.morph_bank_path(SkinSex::Male).is_err());

        fs::write(banks.join("m_mb"), b"male").unwrap();
        assert_eq!(
            opened.morph_bank_path(SkinSex::Male).unwrap(),
            canonical_banks.join("m_mb")
        );
        assert!(opened.morph_bank_path(SkinSex::Unknown).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod body_morph_probe {
    use super::*;

    #[test]
    #[ignore = "reads the user's VaM install"]
    fn the_builtin_catalog_now_carries_the_body() {
        let Some(root) = std::env::var_os("VKIT_VAM_ROOT")
            .map(std::path::PathBuf::from)
            .and_then(|path| VaMRoot::open(&path).ok())
        else {
            eprintln!("set VKIT_VAM_ROOT to run this");
            return;
        };
        let catalog = MorphCatalog::scan_female_builtin(&root).expect("builtin catalog");
        let entries = catalog.entries();
        let mut counts: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        for morph in entries {
            *counts.entry(format!("{:?}", morph.category)).or_default() += 1;
        }
        println!("builtin controls: {}", entries.len());
        for (category, count) in &counts {
            println!("  {count:>5}  {category}");
        }
        let body = counts.get("Body").copied().unwrap_or_default();
        assert!(
            body > 0,
            "a person has a body and the builtin list has to offer it"
        );
    }
}
