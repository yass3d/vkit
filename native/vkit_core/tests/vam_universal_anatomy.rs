use std::path::PathBuf;

use vkit_core::anatomy::{
    EyelidConformationPlan, discover_head_anatomy, propagate_head_anatomy_auto,
};
use vkit_core::formats::{
    canonical_head_groups_cache_path, collect_canonical_head_groups,
    derive_canonical_head_group_faces, embedded_g2f_eye_closed_morph,
    embedded_g2f_eye_closed_morph_for_vam_anchor, load_dsf_path, read_canonical_head_groups,
    topology_digest,
};
use vkit_core::math::Vec3;
use vkit_core::vam::{
    DiscoveredGeometryBase, GeometryBaseRequest, GeometryBaseTier, GeometrySex, VaMRoot,
    discover_geometry_base,
};

fn real_root() -> VaMRoot {
    let path = std::env::var_os("VKIT_VAM_ROOT")
        .map(PathBuf::from)
        .expect("set VKIT_VAM_ROOT to a VaM install");
    VaMRoot::open(path).expect("real VaM integration fixture is unavailable")
}

fn real_dsf_path() -> PathBuf {
    std::env::var_os("VKIT_G2_DSF")
        .map(PathBuf::from)
        .expect("set VKIT_G2_DSF to the licensed Genesis2Female.dsf")
}

struct ScratchDir(PathBuf);

impl ScratchDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "vkit-universal-anatomy-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch cache directory");
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn discover(
    root: &VaMRoot,
    sex: GeometrySex,
    cache_dir: Option<&std::path::Path>,
) -> DiscoveredGeometryBase {
    let discovered = discover_geometry_base(GeometryBaseRequest {
        root: Some(root),
        sex,
        licensed_anchor: None,
        explicit_candidates: &[],
        cache_dir,
    })
    .expect("tiered discovery must enroll from the real install");
    assert!(
        matches!(
            discovered.tier,
            GeometryBaseTier::UnityBundle | GeometryBaseTier::UnityAnchorPair
        ),
        "expected a Tier-1-verified enrollment, got {:?} via {:?}",
        discovered.tier,
        discovered.notes
    );
    discovered
}

#[test]
#[ignore = "requires the user's local VaM installation (VKIT_VAM_ROOT)"]
fn bundle_enrolled_providers_support_head_anatomy_for_both_sexes() {
    let root = real_root();
    let cache = ScratchDir::new("both-sexes");
    for sex in [GeometrySex::Female, GeometrySex::Male] {
        let discovered = discover(&root, sex, Some(cache.path()));
        let anchor = discovered.provider.daz_anchor();

        let components = discover_head_anatomy(anchor)
            .unwrap_or_else(|error| panic!("{sex:?}: head anatomy discovery failed: {error}"));
        assert_eq!(components.left_eye.len(), components.right_eye.len());
        assert!(
            components.left_eye.len() > 300,
            "{sex:?}: eye globe too small"
        );
        assert!(!components.upper_teeth.is_empty(), "{sex:?}");
        assert!(!components.lower_teeth.is_empty(), "{sex:?}");
        assert!(!components.upper_gums.is_empty(), "{sex:?}");
        assert!(!components.lower_gums.is_empty(), "{sex:?}");
        assert!(!components.tongue.is_empty(), "{sex:?}");

        let fitted = anchor
            .vertices
            .iter()
            .copied()
            .map(Vec3::from)
            .collect::<Vec<_>>();
        propagate_head_anatomy_auto(anchor, &fitted)
            .unwrap_or_else(|error| panic!("{sex:?}: identity propagation failed: {error}"));

        EyelidConformationPlan::build(anchor)
            .unwrap_or_else(|error| panic!("{sex:?}: eyelid plan failed: {error}"));
    }

    let path = canonical_head_groups_cache_path(&cache.path().join("neutral-base"));
    assert!(path.is_file(), "canonical anatomy cache receipt missing");
}

#[test]
#[ignore = "requires the user's local VaM installation and licensed G2 DSF"]
fn canonical_groups_match_the_licensed_dsf_and_are_sex_invariant() {
    let dsf = load_dsf_path(real_dsf_path(), 0).expect("licensed DSF fixture");
    let authored = collect_canonical_head_groups(&dsf)
        .expect("licensed DSF carries the authored canonical face groups");
    let derived = derive_canonical_head_group_faces(&dsf).expect("rule derivation on the DSF");

    assert_eq!(derived.left_eye, authored.left_eye);
    assert_eq!(derived.right_eye, authored.right_eye);
    assert_eq!(derived.upper_jaw, authored.upper_jaw);

    let missing_from_lower: Vec<u32> = authored
        .lower_jaw
        .iter()
        .copied()
        .filter(|face| !derived.lower_jaw.contains(face))
        .collect();
    let extra_in_tongue: Vec<u32> = derived
        .tongue
        .iter()
        .copied()
        .filter(|face| !authored.tongue.contains(face))
        .collect();
    assert_eq!(missing_from_lower, extra_in_tongue);
    assert!(
        missing_from_lower.len() <= 8,
        "tongue-root residue grew: {missing_from_lower:?}"
    );
    assert!(
        derived
            .lower_jaw
            .iter()
            .all(|face| authored.lower_jaw.contains(face)),
        "derived lowerJaw may only shrink by the tongue-root bridge"
    );
    for &face in &missing_from_lower {
        let material = &dsf.material_groups[dsf.material_group_indices[face as usize] as usize];
        assert_eq!(material, "Tongue", "residual face {face} is not tongue");
    }

    let root = real_root();
    let cache = ScratchDir::new("dsf-parity");
    let dsf_digest = topology_digest(dsf.vertices.len(), &dsf.faces).unwrap();
    for sex in [GeometrySex::Female, GeometrySex::Male] {
        let discovered = discover(&root, sex, Some(cache.path()));
        let anchor = discovered.provider.daz_anchor();

        assert_eq!(
            topology_digest(anchor.vertices.len(), &anchor.faces).unwrap(),
            dsf_digest,
            "{sex:?}: bundle anchor must share the canonical ordered topology"
        );
        let bundle_groups = collect_canonical_head_groups(anchor)
            .expect("bundle anchor must expose the canonical groups after enrollment");
        assert_eq!(
            bundle_groups, derived,
            "{sex:?}: every anchor source must share one canonical component index"
        );
    }

    let path = canonical_head_groups_cache_path(&cache.path().join("neutral-base"));
    let document = read_canonical_head_groups(
        &path,
        &dsf_digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
        dsf.faces.len(),
    )
    .expect("cached canonical anatomy document");
    assert_eq!(document.face_sets(), derived);
}

#[test]
#[ignore = "requires the user's local VaM installation (VKIT_VAM_ROOT)"]
fn embedded_eye_close_morph_binds_to_the_male_bundle_anchor() {
    let root = real_root();
    let discovered = discover(&root, GeometrySex::Male, None);
    let anchor = discovered.provider.daz_anchor();

    let exact = embedded_g2f_eye_closed_morph(anchor)
        .expect("digest-exact binding on the male bundle anchor");
    let routed = embedded_g2f_eye_closed_morph_for_vam_anchor(anchor)
        .expect("shared-vertex-order binding on the male bundle anchor");
    for (label, morph) in [("exact", &exact), ("routed", &routed)] {
        assert!(
            morph
                .deltas
                .iter()
                .any(|delta| delta.iter().any(|component| *component != 0.0)),
            "{label}: eye-close morph must carry nonzero deltas"
        );
    }
}
