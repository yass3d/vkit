use std::collections::BTreeMap;

use serde_json::Value;

use super::package_index::{PackageAsset, PackageIndex};

#[derive(Clone, Debug, PartialEq)]
pub struct PoseBone {
    pub id: String,

    pub position: Option<[f32; 3]>,

    pub rotation: Option<[f32; 3]>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PoseRoot {
    pub position: Option<[f32; 3]>,
    pub rotation: Option<[f32; 3]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MorphReference {
    Builtin(String),

    Packaged {
        package: Option<PackageRequest>,

        entry: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageRequest {
    pub creator: String,
    pub name: String,

    pub version: Option<u32>,
}

impl PackageRequest {
    fn matches(&self, uid: &str) -> bool {
        let Some((creator, rest)) = uid.split_once('.') else {
            return false;
        };
        let Some((name, version)) = rest.rsplit_once('.') else {
            return false;
        };
        if !creator.eq_ignore_ascii_case(&self.creator) || !name.eq_ignore_ascii_case(&self.name) {
            return false;
        }
        match self.version {
            Some(wanted) => version.parse::<u32>().is_ok_and(|found| found == wanted),
            None => true,
        }
    }

    fn version_of(uid: &str) -> u32 {
        uid.rsplit_once('.')
            .and_then(|(_, version)| version.parse().ok())
            .unwrap_or(0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PoseMorph {
    pub reference: MorphReference,

    pub name: String,
    pub value: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PoseDocument {
    pub root: PoseRoot,
    pub bones: Vec<PoseBone>,
    pub morphs: Vec<PoseMorph>,
}

const fn mirrored_turn([x, y, z]: [f32; 3]) -> [f32; 3] {
    [x, -y, -z]
}

impl PoseDocument {
    #[must_use]
    pub fn to_canonical_space(&self, scale: f32) -> Self {
        Self {
            root: PoseRoot {
                position: self
                    .root
                    .position
                    .map(|[x, y, z]| [-x * scale, y * scale, z * scale]),
                rotation: self.root.rotation.map(mirrored_turn),
            },
            bones: self
                .bones
                .iter()
                .map(|bone| PoseBone {
                    id: bone.id.clone(),
                    position: bone
                        .position
                        .map(|[x, y, z]| [-x * scale, y * scale, z * scale]),
                    rotation: bone.rotation.map(mirrored_turn),
                })
                .collect(),
            morphs: self.morphs.clone(),
        }
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let document: Value = crate::vam::simple_json::parse_document(bytes)
            .map_err(|error| format!("pose preset: {error}"))?;
        let storables = document
            .get("storables")
            .and_then(Value::as_array)
            .ok_or_else(|| "pose preset has no storables array".to_owned())?;

        let mut pose = Self::default();
        for storable in storables {
            let Some(id) = storable.get("id").and_then(Value::as_str) else {
                continue;
            };

            if id.ends_with("Control") {
                continue;
            }
            if id == "geometry" {
                pose.morphs.extend(read_morphs(storable));
                continue;
            }
            let root_position = read_vector(storable.get("rootPosition"));
            let root_rotation = read_vector(storable.get("rootRotation"));
            if root_position.is_some() || root_rotation.is_some() {
                pose.root = PoseRoot {
                    position: root_position,
                    rotation: root_rotation,
                };
                continue;
            }
            let position = read_vector(storable.get("position"));
            let rotation = read_vector(storable.get("rotation"));
            if position.is_some() || rotation.is_some() {
                pose.bones.push(PoseBone {
                    id: id.to_owned(),
                    position,
                    rotation,
                });
            }
        }
        Ok(pose)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PoseMorphSource<'index> {
    Builtin,

    Packaged(&'index PackageAsset),

    Missing,
}

#[must_use]
pub fn resolve_pose_morph<'index>(
    reference: &MorphReference,
    owning_package: Option<&str>,
    index: &'index PackageIndex,
) -> PoseMorphSource<'index> {
    let (package, entry) = match reference {
        MorphReference::Builtin(_) => return PoseMorphSource::Builtin,
        MorphReference::Packaged { package, entry } => (package, entry),
    };

    let mut candidates: Vec<(u32, usize)> = index
        .packages
        .iter()
        .enumerate()
        .filter(|(_, installed)| match package {
            Some(request) => request.matches(&installed.uid),
            None => owning_package.is_some_and(|uid| uid.eq_ignore_ascii_case(&installed.uid)),
        })
        .map(|(ordinal, installed)| (PackageRequest::version_of(&installed.uid), ordinal))
        .collect();
    if candidates.is_empty() {
        return PoseMorphSource::Missing;
    }
    candidates.sort_unstable_by_key(|(version, _)| std::cmp::Reverse(*version));

    for (_, ordinal) in candidates {
        let Ok(ordinal) = u32::try_from(ordinal) else {
            continue;
        };
        if let Some(asset) = index
            .assets
            .iter()
            .find(|asset| asset.package == ordinal && asset.entry.eq_ignore_ascii_case(entry))
        {
            return PoseMorphSource::Packaged(asset);
        }
    }
    PoseMorphSource::Missing
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PoseResolution {
    pub builtin: usize,
    pub resolved: usize,

    pub missing: Vec<String>,
}

#[must_use]
pub fn resolve_pose(
    pose: &PoseDocument,
    owning_package: Option<&str>,
    index: &PackageIndex,
) -> PoseResolution {
    let mut resolution = PoseResolution::default();
    let mut missing: BTreeMap<String, ()> = BTreeMap::new();
    for morph in &pose.morphs {
        match resolve_pose_morph(&morph.reference, owning_package, index) {
            PoseMorphSource::Builtin => resolution.builtin += 1,
            PoseMorphSource::Packaged(_) => resolution.resolved += 1,
            PoseMorphSource::Missing => {
                missing.insert(morph.name.clone(), ());
            }
        }
    }
    resolution.missing = missing.into_keys().collect();
    resolution
}

fn read_morphs(storable: &Value) -> Vec<PoseMorph> {
    let Some(entries) = storable.get("morphs").and_then(Value::as_array) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let uid = entry.get("uid").and_then(Value::as_str)?;
            let value = read_number(entry.get("value"))?;
            let name = entry
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(uid)
                .to_owned();
            Some(PoseMorph {
                reference: parse_reference(uid),
                name,
                value,
            })
        })
        .collect()
}

fn parse_reference(uid: &str) -> MorphReference {
    let Some((package, entry)) = uid.split_once(":/") else {
        return MorphReference::Builtin(uid.to_owned());
    };
    let entry = entry.trim_start_matches('/').to_owned();
    if package.eq_ignore_ascii_case("SELF") {
        return MorphReference::Packaged {
            package: None,
            entry,
        };
    }

    let Some((creator, rest)) = package.split_once('.') else {
        return MorphReference::Builtin(uid.to_owned());
    };
    let (name, version) = rest.rsplit_once('.').unwrap_or((rest, "latest"));
    MorphReference::Packaged {
        package: Some(PackageRequest {
            creator: creator.to_owned(),
            name: name.to_owned(),
            version: version.parse().ok(),
        }),
        entry,
    }
}

fn read_number(value: Option<&Value>) -> Option<f32> {
    match value? {
        Value::Number(number) => {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "preset values are authored at f32 precision"
            )]
            let value = number.as_f64()? as f32;
            value.is_finite().then_some(value)
        }
        Value::String(text) => text
            .trim()
            .parse()
            .ok()
            .filter(|value: &f32| value.is_finite()),
        _ => None,
    }
}

fn read_vector(value: Option<&Value>) -> Option<[f32; 3]> {
    let value = value?;
    Some([
        read_number(value.get("x"))?,
        read_number(value.get("y"))?,
        read_number(value.get("z"))?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "setUnlistedParamsToDefault": "true",
        "storables": [
            {"id": "headControl", "position": {"x": "9", "y": "9", "z": "9"},
             "rotation": {"x": "9", "y": "9", "z": "9"}, "canGrabPosition": "true"},
            {"id": "hip", "rootPosition": {"x": "0", "y": "1.2", "z": "-0.4"},
             "rootRotation": {"x": "0", "y": "12", "z": "0"}},
            {"id": "rThigh", "position": {"x": "0", "y": "0", "z": "0"},
             "rotation": {"x": "-14.5", "y": "2", "z": "1"}},
            {"id": "lForeArm", "rotation": {"x": "40", "y": "0", "z": "0"}},
            {"id": "geometry", "morphs": [
                {"uid": "Left Fingers Fist", "name": "Left Fingers Fist", "value": "1"},
                {"uid": "SELF:/Custom/Atom/Person/Morphs/female/Me/Smile.vmi",
                 "name": "Smile", "value": "0.5"},
                {"uid": "Jackaroo.JarModularExpressions.latest:/Custom/Atom/Person/Morphs/female/J/BrowU_C.vmi",
                 "name": "BrowU_C", "value": "0.25"},
                {"uid": "Someone.Gone.2:/Custom/Atom/Person/Morphs/female/G/Nope.vmi",
                 "name": "Nope", "value": "0.9"}
            ]}
        ]
    }"#;

    #[test]
    fn a_preset_separates_its_bones_its_root_and_its_morphs() {
        let pose = PoseDocument::parse(SAMPLE.as_bytes()).expect("parses");
        assert_eq!(pose.bones.len(), 2, "{:?}", pose.bones);
        assert!(pose.bones.iter().all(|bone| !bone.id.ends_with("Control")));
        assert_eq!(pose.bones[0].id, "rThigh");
        assert_eq!(pose.bones[0].rotation, Some([-14.5, 2.0, 1.0]));

        assert_eq!(pose.bones[1].id, "lForeArm");
        assert_eq!(pose.bones[1].position, None);
        assert_eq!(pose.root.position, Some([0.0, 1.2, -0.4]));
        assert_eq!(pose.morphs.len(), 4);
    }

    #[test]
    fn the_three_ways_of_naming_a_morph_are_told_apart() {
        let pose = PoseDocument::parse(SAMPLE.as_bytes()).expect("parses");
        assert_eq!(
            pose.morphs[0].reference,
            MorphReference::Builtin("Left Fingers Fist".to_owned())
        );
        assert_eq!(
            pose.morphs[1].reference,
            MorphReference::Packaged {
                package: None,
                entry: "Custom/Atom/Person/Morphs/female/Me/Smile.vmi".to_owned(),
            }
        );
        let MorphReference::Packaged {
            package: Some(request),
            ..
        } = &pose.morphs[2].reference
        else {
            panic!("expected a packaged reference: {:?}", pose.morphs[2]);
        };
        assert_eq!(request.creator, "Jackaroo");
        assert_eq!(request.name, "JarModularExpressions");
        assert_eq!(request.version, None, "`latest` pins no version");
        assert_eq!(
            pose.morphs
                .iter()
                .filter(|morph| matches!(morph.reference, MorphReference::Packaged { .. }))
                .count(),
            3
        );
    }

    #[test]
    fn a_package_name_with_dots_in_it_keeps_them() {
        let MorphReference::Packaged {
            package: Some(request),
            entry,
        } = parse_reference("Creator.Pack.v1.2.3:/Custom/Atom/Person/Morphs/female/A.vmi")
        else {
            panic!("expected a packaged reference");
        };
        assert_eq!(request.creator, "Creator");
        assert_eq!(request.name, "Pack.v1.2");
        assert_eq!(request.version, Some(3));
        assert_eq!(entry, "Custom/Atom/Person/Morphs/female/A.vmi");
    }

    fn library() -> PackageIndex {
        let json = serde_json::json!({
            "packages": [
                {"path": "a.var", "uid": "Jackaroo.JarModularExpressions.2",
                 "len": 1, "modified_ms": 1},
                {"path": "b.var", "uid": "Jackaroo.JarModularExpressions.7",
                 "len": 1, "modified_ms": 1},
                {"path": "c.var", "uid": "Me.MyPose.1", "len": 1, "modified_ms": 1}
            ],
            "assets": [
                {"package": 0, "kind": "Morph",
                 "entry": "Custom/Atom/Person/Morphs/female/J/BrowU_C.vmi"},
                {"package": 1, "kind": "Morph",
                 "entry": "Custom/Atom/Person/Morphs/female/J/BrowU_C.vmi"},
                {"package": 2, "kind": "Morph",
                 "entry": "Custom/Atom/Person/Morphs/female/Me/Smile.vmi"}
            ],
            "unreadable": []
        });
        serde_json::from_value(json).expect("index fixture")
    }

    #[test]
    fn latest_means_the_newest_installed_version() {
        let index = library();
        let pose = PoseDocument::parse(SAMPLE.as_bytes()).expect("parses");
        let PoseMorphSource::Packaged(asset) =
            resolve_pose_morph(&pose.morphs[2].reference, Some("Me.MyPose.1"), &index)
        else {
            panic!("the expression pack is installed and should resolve");
        };
        assert_eq!(
            index.package_of(asset).map(|package| package.uid.as_str()),
            Some("Jackaroo.JarModularExpressions.7")
        );
    }

    #[test]
    fn self_means_the_package_the_pose_came_from() {
        let index = library();
        let pose = PoseDocument::parse(SAMPLE.as_bytes()).expect("parses");
        assert!(matches!(
            resolve_pose_morph(&pose.morphs[1].reference, Some("Me.MyPose.1"), &index),
            PoseMorphSource::Packaged(_)
        ));
        assert!(matches!(
            resolve_pose_morph(
                &pose.morphs[1].reference,
                Some("Jackaroo.JarModularExpressions.7"),
                &index
            ),
            PoseMorphSource::Missing
        ));
        assert!(matches!(
            resolve_pose_morph(&pose.morphs[1].reference, None, &index),
            PoseMorphSource::Missing
        ));
    }

    #[test]
    fn a_missing_expression_pack_is_named_and_not_swallowed() {
        let index = library();
        let pose = PoseDocument::parse(SAMPLE.as_bytes()).expect("parses");
        let resolution = resolve_pose(&pose, Some("Me.MyPose.1"), &index);
        assert_eq!(resolution.builtin, 1);
        assert_eq!(resolution.resolved, 2);
        assert_eq!(resolution.missing, vec!["Nope".to_owned()]);
    }

    #[test]
    fn a_pinned_version_does_not_drift_to_the_newest() {
        let index = library();
        let pinned = parse_reference(
            "Jackaroo.JarModularExpressions.2:/Custom/Atom/Person/Morphs/female/J/BrowU_C.vmi",
        );
        let PoseMorphSource::Packaged(asset) = resolve_pose_morph(&pinned, None, &index) else {
            panic!("version 2 is installed");
        };
        assert_eq!(
            index.package_of(asset).map(|package| package.uid.as_str()),
            Some("Jackaroo.JarModularExpressions.2")
        );

        let absent = parse_reference(
            "Jackaroo.JarModularExpressions.9:/Custom/Atom/Person/Morphs/female/J/BrowU_C.vmi",
        );
        assert!(matches!(
            resolve_pose_morph(&absent, None, &index),
            PoseMorphSource::Missing
        ));
    }

    #[test]
    fn a_value_is_read_whether_or_not_it_is_quoted() {
        let document = r#"{"storables": [
            {"id": "geometry", "morphs": [
                {"uid": "A", "value": 0.25},
                {"uid": "B", "value": "0.5"},
                {"uid": "C", "value": "not a number"},
                {"uid": "D", "value": "NaN"},
                {"uid": "E"}
            ]},
            {"id": "bone", "rotation": {"x": 1, "y": "2", "z": 3.5}}
        ]}"#;
        let pose = PoseDocument::parse(document.as_bytes()).expect("parses");
        assert_eq!(pose.morphs.len(), 2, "{:?}", pose.morphs);
        assert!((pose.morphs[0].value - 0.25).abs() < f32::EPSILON);
        assert!((pose.morphs[1].value - 0.5).abs() < f32::EPSILON);
        assert_eq!(pose.bones[0].rotation, Some([1.0, 2.0, 3.5]));
    }

    #[test]
    fn a_leading_byte_order_mark_does_not_stop_the_read() {
        let with_mark = format!("\u{feff}{SAMPLE}");
        assert!(PoseDocument::parse(with_mark.as_bytes()).is_ok());
    }

    #[test]
    #[ignore = "reads the user's VaM install"]
    fn every_pose_in_the_real_library_reads() {
        let Some(dir) = std::env::var_os("VKIT_ADDON_PACKAGES").map(std::path::PathBuf::from)
        else {
            eprintln!("set VKIT_ADDON_PACKAGES to run this");
            return;
        };
        let index = PackageIndex::build(&dir, None, |_| {});
        let poses: Vec<&PackageAsset> = index
            .of_kind(super::super::PackageAssetKind::Pose)
            .filter(|asset| asset.entry.to_ascii_lowercase().ends_with(".vap"))
            .collect();
        assert!(!poses.is_empty(), "the library should hold poses");

        let (mut read, mut failed, mut bones, mut with_morphs) = (0, 0, 0, 0);
        let (mut builtin, mut resolved) = (0usize, 0usize);
        let mut missing_packs: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();

        for asset in poses.iter().step_by(7).take(600) {
            let Some(package) = index.package_of(asset) else {
                continue;
            };
            let Ok(bytes) =
                super::super::read_var_entry_bytes(&package.path, &asset.entry, 8 * 1024 * 1024)
            else {
                failed += 1;
                continue;
            };
            match PoseDocument::parse(&bytes) {
                Ok(pose) => {
                    read += 1;
                    bones += pose.bones.len();
                    if !pose.morphs.is_empty() {
                        with_morphs += 1;
                    }
                    let outcome = resolve_pose(&pose, Some(&package.uid), &index);
                    builtin += outcome.builtin;
                    resolved += outcome.resolved;
                    for name in outcome.missing {
                        *missing_packs.entry(name).or_default() += 1;
                    }
                }
                Err(_) => failed += 1,
            }
        }
        println!(
            "{read} poses read, {failed} failed, {bones} bone entries, {with_morphs} carry morphs"
        );
        println!("morph refs: {builtin} built in, {resolved} resolved from packages");
        println!("unresolved morph names: {}", missing_packs.len());
        for (name, count) in missing_packs.iter().take(8) {
            println!("  {count:>4} x {name}");
        }
        assert!(read > 0, "at least some poses must read");
        assert!(
            failed * 20 < read,
            "{failed} failures against {read} reads is too many for a format this uniform"
        );
    }

    #[test]
    fn what_is_not_a_pose_says_so() {
        assert!(PoseDocument::parse(b"not json").is_err());
        assert!(PoseDocument::parse(br#"{"other": []}"#).is_err());

        let controls = br#"{"storables": [{"id": "headControl", "position": {"x": "1", "y": "1", "z": "1"}}]}"#;
        let pose = PoseDocument::parse(controls).expect("a control-only preset is still a preset");
        assert!(pose.bones.is_empty() && pose.morphs.is_empty());
    }
}
