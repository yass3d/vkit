use std::path::{Path, PathBuf};

use crate::formats::{
    CanonicalHeadGroupsDocument, DazGeometry, canonical_head_groups_cache_path,
    collect_canonical_head_groups, load_ordered_obj, read_canonical_head_groups, topology_digest,
    write_canonical_head_groups,
};

use super::catalog::VaMRoot;
use super::geometry::{
    GeometrySex, NeutralityReceipt, VaMGeometryProvider, validate_neutral_shared_prefix,
};
use super::unity_base::{
    NeutralBaseHandle, hex, load_or_extract_neutral_base, neutral_cache_directory,
};

const MAX_NOTES: usize = 12;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeometryBaseTier {
    UnityBundle,

    DazAnchorPair,

    UnityAnchorPair,

    UnverifiedUserObj,
}

#[derive(Clone, Copy)]
pub struct GeometryBaseRequest<'a> {
    pub root: Option<&'a VaMRoot>,
    pub sex: GeometrySex,

    pub licensed_anchor: Option<&'a DazGeometry>,

    pub explicit_candidates: &'a [PathBuf],

    pub cache_dir: Option<&'a Path>,
}

#[derive(Clone, Debug)]
pub struct DiscoveredGeometryBase {
    pub provider: VaMGeometryProvider,

    pub base_path: PathBuf,
    pub tier: GeometryBaseTier,

    pub neutrality: Option<NeutralityReceipt>,
    pub neutral_bundle: Option<PathBuf>,
    pub neutral_from_cache: Option<bool>,

    pub notes: Vec<String>,
}

pub fn discover_geometry_base(
    request: GeometryBaseRequest<'_>,
) -> Result<DiscoveredGeometryBase, String> {
    let mut notes = Vec::new();
    let neutral = load_neutral(&request, &mut notes);
    let neutral_provider = neutral.as_ref().and_then(|handle| {
        match VaMGeometryProvider::from_neutral_base(request.sex, handle.base.mesh.clone()) {
            Ok(provider) => Some(provider),
            Err(error) => {
                push_note(
                    &mut notes,
                    format!(
                        "{}: extracted neutral base failed provider gates: {error}",
                        handle.base.bundle_path.display()
                    ),
                );
                None
            }
        }
    });
    if let (Some(provider), Some(handle), Some(cache_dir)) =
        (&neutral_provider, &neutral, request.cache_dir)
    {
        persist_canonical_head_groups(cache_dir, provider, handle);
    }

    let mut explicit = Vec::new();
    for path in request.explicit_candidates {
        push_candidate(&mut explicit, path.clone());
    }
    let mut scanned = Vec::new();
    if let Some(root) = request.root {
        for path in root.geometry_base_candidates(request.sex) {
            if !explicit.contains(&path) {
                push_candidate(&mut scanned, path);
            }
        }
    }

    let enroll = |candidates: &[PathBuf],
                  neutral_provider: Option<&VaMGeometryProvider>,
                  notes: &mut Vec<String>|
     -> Option<DiscoveredGeometryBase> {
        for path in candidates {
            let Some((provider, tier, neutrality)) =
                enroll_candidate(&request, path, neutral.as_ref(), neutral_provider, notes)
            else {
                continue;
            };
            return Some(DiscoveredGeometryBase {
                provider,
                base_path: path.clone(),
                tier,
                neutrality,
                neutral_bundle: neutral
                    .as_ref()
                    .map(|handle| handle.base.bundle_path.clone()),
                neutral_from_cache: neutral.as_ref().map(|handle| handle.from_cache),
                notes: std::mem::take(notes),
            });
        }
        None
    };

    if let Some(found) = enroll(&explicit, neutral_provider.as_ref(), &mut notes) {
        return Ok(found);
    }

    if let (Some(provider), Some(handle)) = (neutral_provider.as_ref(), neutral.as_ref()) {
        let usable = request
            .licensed_anchor
            .is_none_or(|anchor| anchor_matches_canonical(anchor, provider));
        if usable {
            return Ok(DiscoveredGeometryBase {
                base_path: handle.base.bundle_path.clone(),
                tier: GeometryBaseTier::UnityBundle,
                neutrality: None,
                neutral_bundle: Some(handle.base.bundle_path.clone()),
                neutral_from_cache: Some(handle.from_cache),
                notes: std::mem::take(&mut notes),
                provider: provider.clone(),
            });
        }
        push_note(
            &mut notes,
            "the loaded template topology is not the canonical Genesis 2 stream, so the Unity-bundle provider cannot back it".to_owned(),
        );
    }

    if let Some(found) = enroll(&scanned, neutral_provider.as_ref(), &mut notes) {
        return Ok(found);
    }

    if notes.is_empty() {
        notes.push(
            "select a VaM installation folder or a user-owned neutral full-body OBJ".to_owned(),
        );
    }
    Err(notes.join("; "))
}

fn load_neutral(
    request: &GeometryBaseRequest<'_>,
    notes: &mut Vec<String>,
) -> Option<NeutralBaseHandle> {
    let root = request.root?;
    match load_or_extract_neutral_base(root, request.sex, request.cache_dir) {
        Ok(handle) => Some(handle),
        Err(error) => {
            push_note(
                notes,
                format!("neutral base extraction unavailable: {error}"),
            );
            None
        }
    }
}

fn enroll_candidate(
    request: &GeometryBaseRequest<'_>,
    path: &Path,
    neutral: Option<&NeutralBaseHandle>,
    neutral_provider: Option<&VaMGeometryProvider>,
    notes: &mut Vec<String>,
) -> Option<(
    VaMGeometryProvider,
    GeometryBaseTier,
    Option<NeutralityReceipt>,
)> {
    let basis = match load_ordered_obj(path) {
        Ok(basis) => basis,
        Err(error) => {
            push_note(notes, format!("{}: {error}", path.display()));
            return None;
        }
    };
    match GeometrySex::shortlist_from_counts(basis.vertices.len(), basis.faces.len()) {
        Some(sex) if sex == request.sex => {}
        _ => {
            push_note(
                notes,
                format!(
                    "{}: {} vertices / {} polygons is not a {} VaM full mesh",
                    path.display(),
                    basis.vertices.len(),
                    basis.faces.len(),
                    sex_label(request.sex)
                ),
            );
            return None;
        }
    }
    let neutrality = match neutral {
        Some(handle) => match validate_neutral_shared_prefix(&basis, &handle.base.mesh) {
            Ok(receipt) => Some(receipt),
            Err(error) => {
                push_note(notes, format!("{}: {error}", path.display()));
                return None;
            }
        },
        None => None,
    };

    let (anchor, tier, anchor_is_vam) = if let Some(anchor) = request.licensed_anchor {
        (anchor, GeometryBaseTier::DazAnchorPair, false)
    } else if let Some(provider) = neutral_provider {
        (
            provider.daz_anchor(),
            GeometryBaseTier::UnityAnchorPair,
            true,
        )
    } else {
        return match VaMGeometryProvider::from_vam_basis(request.sex, basis) {
            Ok(provider) => Some((provider, GeometryBaseTier::UnverifiedUserObj, neutrality)),
            Err(error) => {
                push_note(notes, format!("{}: {error}", path.display()));
                None
            }
        };
    };
    let built = if anchor_is_vam {
        VaMGeometryProvider::from_vam_anchored_pair(request.sex, anchor.clone(), basis)
    } else {
        VaMGeometryProvider::from_user_owned_pair(request.sex, anchor.clone(), basis)
    };
    match built {
        Ok(provider) => Some((provider, tier, neutrality)),
        Err(error) => {
            push_note(notes, format!("{}: {error}", path.display()));
            None
        }
    }
}

fn persist_canonical_head_groups(
    cache_dir: &Path,
    provider: &VaMGeometryProvider,
    handle: &NeutralBaseHandle,
) {
    let Some(faces) = collect_canonical_head_groups(provider.daz_anchor()) else {
        return;
    };
    let topology_hex = hex(&handle.base.topology_sha256);
    let source = format!(
        "unity-bundle:{}#sha256:{}",
        handle.base.bundle_path.display(),
        hex(&handle.base.bundle_sha256)
    );
    let document = CanonicalHeadGroupsDocument::new(topology_hex.clone(), source, &faces);
    let path = canonical_head_groups_cache_path(&neutral_cache_directory(cache_dir));

    if read_canonical_head_groups(&path, &topology_hex, provider.daz_anchor().faces.len())
        .is_ok_and(|existing| existing.face_sets() == faces)
    {
        return;
    }
    let _ = write_canonical_head_groups(&path, &document);
}

fn anchor_matches_canonical(anchor: &DazGeometry, provider: &VaMGeometryProvider) -> bool {
    let provider_anchor = provider.daz_anchor();
    anchor.vertices.len() == provider_anchor.vertices.len()
        && topology_digest(anchor.vertices.len(), &anchor.faces).ok()
            == topology_digest(provider_anchor.vertices.len(), &provider_anchor.faces).ok()
}

fn push_candidate(candidates: &mut Vec<PathBuf>, path: PathBuf) {
    let is_obj = path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("obj"));
    if is_obj && !candidates.contains(&path) {
        candidates.push(path);
    }
}

fn push_note(notes: &mut Vec<String>, note: String) {
    if notes.len() < MAX_NOTES {
        notes.push(note);
    }
}

const fn sex_label(sex: GeometrySex) -> &'static str {
    match sex {
        GeometrySex::Female => "female",
        GeometrySex::Male => "male",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch_root(label: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "vkit-base-discovery-{label}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(directory.join("VaM_Data").join("StreamingAssets")).unwrap();
        directory
    }

    #[test]
    fn empty_request_returns_an_actionable_error() {
        let error = discover_geometry_base(GeometryBaseRequest {
            root: None,
            sex: GeometrySex::Male,
            licensed_anchor: None,
            explicit_candidates: &[],
            cache_dir: None,
        })
        .unwrap_err();
        assert!(error.contains("select a VaM installation"), "{error}");
    }

    #[test]
    fn missing_bundle_and_character_export_produce_a_bounded_note_trail() {
        let directory = scratch_root("notes");

        let export = directory.join("male2-Hector.obj");
        fs::write(&export, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let root = VaMRoot::open(&directory).unwrap();

        let error = discover_geometry_base(GeometryBaseRequest {
            root: Some(&root),
            sex: GeometrySex::Male,
            licensed_anchor: None,
            explicit_candidates: &[],
            cache_dir: None,
        })
        .unwrap_err();
        assert!(
            error.contains("neutral base extraction unavailable"),
            "{error}"
        );
        assert!(error.contains("male2-Hector.obj"), "{error}");
        assert!(error.contains("not a male VaM full mesh"), "{error}");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn explicit_candidates_are_deduplicated_and_restricted_to_obj() {
        let mut candidates = Vec::new();
        push_candidate(&mut candidates, PathBuf::from("a.obj"));
        push_candidate(&mut candidates, PathBuf::from("a.obj"));
        push_candidate(&mut candidates, PathBuf::from("b.glb"));
        push_candidate(&mut candidates, PathBuf::from("c.OBJ"));
        assert_eq!(candidates, [PathBuf::from("a.obj"), PathBuf::from("c.OBJ")]);
    }

    #[test]
    #[ignore = "requires the user's VaM installation"]
    fn real_vam_root_discovers_a_neutral_verified_provider_for_both_sexes() {
        let root =
            VaMRoot::open(std::env::var_os("VKIT_VAM_ROOT").expect("set VKIT_VAM_ROOT")).unwrap();
        for sex in [GeometrySex::Female, GeometrySex::Male] {
            let discovered = discover_geometry_base(GeometryBaseRequest {
                root: Some(&root),
                sex,
                licensed_anchor: None,
                explicit_candidates: &[],
                cache_dir: None,
            })
            .unwrap();
            assert!(
                matches!(
                    discovered.tier,
                    GeometryBaseTier::UnityBundle | GeometryBaseTier::UnityAnchorPair
                ),
                "expected a Tier-1-verified enrollment, got {:?} via {:?}",
                discovered.tier,
                discovered.notes
            );
            assert_eq!(discovered.provider.sex(), sex);
            assert_eq!(
                discovered.provider.daz_anchor().vertices.len(),
                crate::G2F_VERTEX_COUNT
            );
            assert_eq!(
                discovered.provider.daz_anchor().faces.len(),
                crate::G2F_POLYGON_COUNT
            );
        }
    }
}

#[cfg(test)]
mod ranking {
    use super::*;

    #[test]
    #[ignore = "reads the reader's own VaM installation"]
    fn nothing_found_in_the_folder_is_opened_while_the_bundle_can_answer() {
        let Ok(source) = std::env::var("VKIT_VAM_ROOT") else {
            return;
        };
        let source = std::path::Path::new(&source);
        let staging = std::env::temp_dir().join(format!("vkit-rank-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&staging);
        let streaming = staging.join("VaM_Data").join("StreamingAssets");
        std::fs::create_dir_all(&streaming).unwrap();
        let bundle = source.join("VaM_Data").join("StreamingAssets").join("f_1");
        std::fs::copy(&bundle, streaming.join("f_1")).unwrap();

        let loose = staging.join("femalecustom.obj");
        std::fs::write(&loose, b"not a mesh").unwrap();
        let root = crate::vam::VaMRoot::open(&staging).unwrap();
        assert!(
            root.geometry_base_candidates(GeometrySex::Female)
                .iter()
                .any(|path| path.ends_with("femalecustom.obj")),
            "the scan should offer the file this test just wrote"
        );

        let request = GeometryBaseRequest {
            root: Some(&root),
            sex: GeometrySex::Female,
            licensed_anchor: None,
            explicit_candidates: &[],
            cache_dir: None,
        };
        let found = discover_geometry_base(request).unwrap();
        assert_eq!(found.tier, GeometryBaseTier::UnityBundle);
        assert_eq!(
            found.provider.vam_basis().vertices.len(),
            crate::G2F_VERTEX_COUNT
        );
        assert!(
            !found.notes.iter().any(|note| note.contains("femalecustom")),
            "the loose mesh was opened before the bundle answered: {:?}",
            found.notes
        );

        let unusable = [loose.clone(), staging.join("absent.obj")];
        let chosen = discover_geometry_base(GeometryBaseRequest {
            explicit_candidates: &unusable,
            ..request
        })
        .unwrap();
        assert_eq!(chosen.tier, GeometryBaseTier::UnityBundle);
        assert!(
            chosen
                .notes
                .iter()
                .any(|note| note.contains("femalecustom")),
            "a candidate named on purpose should be tried and reported on: {:?}",
            chosen.notes
        );

        let _ = std::fs::remove_dir_all(&staging);
    }

    #[test]
    fn a_mesh_the_scan_accepts_is_never_the_shape_the_bundle_carries() {
        for (vertices, faces) in [
            (
                super::super::geometry::VAM_FEMALE_VERTEX_COUNT,
                super::super::geometry::VAM_FEMALE_FACE_COUNT,
            ),
            (
                super::super::geometry::VAM_MALE_VERTEX_COUNT,
                super::super::geometry::VAM_MALE_FACE_COUNT,
            ),
        ] {
            assert!(GeometrySex::shortlist_from_counts(vertices, faces).is_some());
            assert_ne!(vertices, crate::G2F_VERTEX_COUNT);
            assert_ne!(faces, crate::G2F_POLYGON_COUNT);
        }
        assert!(
            GeometrySex::shortlist_from_counts(crate::G2F_VERTEX_COUNT, crate::G2F_POLYGON_COUNT)
                .is_none(),
            "the canonical body is not what the folder scan is looking for"
        );
    }
}
