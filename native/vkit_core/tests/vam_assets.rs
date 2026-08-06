use std::path::{Path, PathBuf};
use std::time::Instant;

use vkit_core::formats::load_dsf_path;
use vkit_core::vam::{
    BuiltinMorphSession, MorphCatalog, VaMRoot, canonical_head_vertex_mask, load_g2_uv_mapping,
    scan_skin_library_with_report,
};

const VAM_ROOT_VAR: &str = "VKIT_VAM_ROOT";
const G2F_DSF_VAR: &str = "VKIT_G2_DSF";

fn fixture_path(variable: &str) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {variable} to run this integration fixture"))
}

fn local_root() -> VaMRoot {
    VaMRoot::open(fixture_path(VAM_ROOT_VAR)).expect("real VaM integration fixture is unavailable")
}

#[test]
#[ignore = "requires a real VaM install; see VKIT_VAM_ROOT"]
fn scans_real_female_builtin_bank_and_resolves_a_head_morph() {
    let root = local_root();
    let catalog_started = Instant::now();
    let session = BuiltinMorphSession::open(&root).unwrap();
    let catalog = MorphCatalog::from_session(&session);
    let catalog_elapsed = catalog_started.elapsed();
    assert!(catalog.entries().len() > 100);
    assert!(
        catalog
            .entries()
            .iter()
            .any(|entry| entry.category == vkit_core::vam::MorphCategory::Brows),
        "real VaM catalog should expose brow controls in the Brows category"
    );
    assert!(
        catalog
            .query(None, "")
            .iter()
            .all(|entry| !entry.is_pose_control && !entry.diagnostic_hidden)
    );
    for required in [
        "CTRLEyesWidth",
        "CTRLEyesDepth",
        "CTRLEyesHeight",
        "CTRLEyesAlmondInner",
        "CTRLEyesAlmondOuter",
        "PHMMouthOpenWide",
    ] {
        assert!(
            catalog
                .entries()
                .iter()
                .any(|entry| entry.internal_name == required),
            "missing {required}"
        );
    }

    let geometry = load_dsf_path(fixture_path(G2F_DSF_VAR), 0).unwrap();
    let mask = canonical_head_vertex_mask(&geometry).unwrap();
    let source = &catalog
        .entries()
        .iter()
        .find(|entry| entry.internal_name == "CTRLEyesWidth")
        .unwrap()
        .source;
    let descriptor = catalog
        .entries()
        .iter()
        .find(|entry| entry.internal_name == "CTRLEyesWidth")
        .unwrap();
    assert!(descriptor.minimum.is_finite());
    assert!(descriptor.maximum.is_finite());
    assert!((descriptor.minimum..=descriptor.maximum).contains(&descriptor.default));
    let load_started = Instant::now();
    let resolved = session.load_resolved(source, &mask).unwrap();
    let load_elapsed = load_started.elapsed();
    let second_source = &catalog
        .entries()
        .iter()
        .find(|entry| entry.internal_name == "CTRLEyesDepth")
        .unwrap()
        .source;
    let second_started = Instant::now();
    let second = session.load_resolved(second_source, &mask).unwrap();
    let second_elapsed = second_started.elapsed();
    println!(
        "real f_mb session+catalog={catalog_elapsed:?}, first morph={load_elapsed:?}, second morph={second_elapsed:?}, entries={}",
        catalog.entries().len()
    );
    assert!(!resolved.sparse_deltas.is_empty());
    assert_eq!(resolved.vertex_count, geometry.vertices.len());
    assert!(resolved.receipt.cyclic_dependencies.is_empty());
    assert!(resolved.receipt.missing_morph_targets.is_empty());
    assert!(!second.sparse_deltas.is_empty());
}

#[test]
#[ignore = "requires a real VaM install; see VKIT_VAM_ROOT"]
fn validates_real_female_custom_uv_prefix() {
    let root = local_root();
    let geometry = load_dsf_path(fixture_path(G2F_DSF_VAR), 0).unwrap();
    let mapping = load_g2_uv_mapping(&root, &geometry).unwrap();
    assert!(mapping.coordinate_rms_cm <= 0.25);
    assert!(mapping.coordinate_max_cm <= 2.0);
    assert!(!mapping.triangles.is_empty());
}

#[test]
#[ignore = "walks a real VaM AddonPackages library; see VKIT_VAM_ROOT"]
fn scans_real_skin_library_without_bundling_assets() {
    let root = local_root();
    let report = scan_skin_library_with_report(&root).unwrap();
    println!(
        "real skin presets={}, skipped_archives={}, skipped_presets={}",
        report.presets.len(),
        report.diagnostics.skipped_archives,
        report.diagnostics.skipped_presets
    );
    for warning in report.diagnostics.warnings.iter().take(8) {
        println!("skin warning: {warning}");
    }
    let skins = report.presets;
    assert!(!skins.is_empty());
    assert!(
        skins
            .iter()
            .any(|skin| skin.diffuse(vkit_core::vam::SkinRegion::Face).is_some())
    );
    for skin in skins.iter().take(100) {
        assert!(!skin.stable_id.is_empty());
        assert!(!skin.label.is_empty());
    }
}

#[test]
fn integration_fixture_paths_are_named_by_the_environment() {
    let source = include_str!("vam_assets.rs");

    let windows_root = format!(":{}", char::from(0x5c));
    let posix_home = format!("{}home{}", '/', '/');
    let posix_users = format!("{}Users{}", '/', '/');
    let mut prefixes: Vec<String> = ('C'..='H')
        .map(|letter| format!("{letter}{windows_root}"))
        .collect();
    prefixes.push(posix_home);
    prefixes.push(posix_users);
    for prefix in prefixes {
        assert!(
            !source.contains(&prefix),
            "an absolute path starting {prefix} is written into this file"
        );
    }
    for variable in [VAM_ROOT_VAR, G2F_DSF_VAR] {
        if let Some(value) = std::env::var_os(variable) {
            assert!(
                Path::new(&value).is_absolute(),
                "{variable} must be an absolute path"
            );
        }
    }
}

#[test]
#[ignore = "requires a real VaM install; see VKIT_VAM_ROOT"]
fn the_g2_face_atlas_is_symmetric_about_u_half() {
    use vkit_core::symmetry::MirrorMap;
    use vkit_core::vam::UvMaterialRegion;

    let root = local_root();
    let geometry = load_dsf_path(fixture_path(G2F_DSF_VAR), 0).unwrap();
    let mapping = load_g2_uv_mapping(&root, &geometry).unwrap();

    let extent = geometry
        .vertices
        .iter()
        .fold((f64::MAX, f64::MIN), |(low, high), vertex| {
            (low.min(vertex[0]), high.max(vertex[0]))
        });
    let plane_x = (extent.0 + extent.1) * 0.5;
    let mirror = MirrorMap::build(&geometry.vertices, plane_x);
    println!(
        "mesh: {} of {} vertices paired, plane_x={plane_x:.4}",
        mirror.paired(),
        geometry.vertices.len()
    );

    let mut uv_of: std::collections::HashMap<u32, [f32; 2]> = std::collections::HashMap::new();
    for triangle in &mapping.triangles {
        if triangle.material_region != UvMaterialRegion::Face {
            continue;
        }
        for (slot, vertex) in triangle.position_indices.iter().enumerate() {
            uv_of.entry(*vertex).or_insert(triangle.uvs[slot]);
        }
    }
    let (mut checked, mut worst_u, mut worst_v, mut sum_u) = (0_usize, 0.0_f32, 0.0_f32, 0.0_f64);
    for (vertex, uv) in &uv_of {
        let Some(partner) = mirror.partner_of(*vertex as usize) else {
            continue;
        };
        let Some(other) = uv_of.get(&(partner as u32)) else {
            continue;
        };
        let du = ((1.0 - uv[0]) - other[0]).abs();
        let dv = (uv[1] - other[1]).abs();
        checked += 1;
        worst_u = worst_u.max(du);
        worst_v = worst_v.max(dv);
        sum_u += f64::from(du);
    }
    let mean_u = sum_u / checked.max(1) as f64;
    println!(
        "atlas: {checked} face vertices compared, mean |Δu|={mean_u:.5}, worst |Δu|={worst_u:.5}, worst |Δv|={worst_v:.5}"
    );
    assert_eq!(mirror.paired(), geometry.vertices.len());
    assert!(
        checked > 2_000,
        "too few face vertices to conclude anything"
    );

    assert!(mean_u < 0.001, "mean |Δu| {mean_u}");
    assert!(worst_u < 0.01, "worst |Δu| {worst_u}");
    assert!(worst_v < 0.01, "worst |Δv| {worst_v}");
}
