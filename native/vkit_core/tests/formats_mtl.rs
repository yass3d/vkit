use std::io::Cursor;
use std::path::PathBuf;

use vkit_core::formats::{DiffuseMapBinding, DiffuseMapReport, MtlOpacitySource, parse_mtl};

#[test]
fn mtl_parses_safe_subset_and_groups_one_shared_atlas() {
    let source = concat!(
        "\u{feff}# exporter metadata\n",
        "newmtl Skin\n",
        "Ka 0 0 0\n",
        "Kd 0.25 0.5 1\n",
        "Tr 0.25\n",
        "map_Kd textures/face diffuse.png\n",
        "newmtl Lips\n",
        "Kd 1 0.1 0.2\n",
        "d 0.8\n",
        "map_Kd textures/face diffuse.png\n",
    );
    let document = parse_mtl(Cursor::new(source)).expect("safe MTL");
    assert_eq!(document.materials.len(), 2);
    assert_eq!(document.materials[0].diffuse_color, Some([0.25, 0.5, 1.0]));
    assert_eq!(document.materials[0].opacity, Some(0.75));
    assert_eq!(
        document.materials[0].opacity_source,
        Some(MtlOpacitySource::Transparency)
    );
    assert_eq!(document.materials[1].opacity, Some(0.8));
    assert_eq!(
        document.diffuse_map_report(),
        DiffuseMapReport::SingleMap {
            path: PathBuf::from("textures").join("face diffuse.png"),
            materials: vec!["Skin".to_owned(), "Lips".to_owned()],
        }
    );
}

#[test]
fn mtl_reports_no_map_and_multiple_maps_without_hiding_materials() {
    let no_map = parse_mtl(Cursor::new("newmtl Skin\nKd 1 1 1\n")).expect("no-map MTL");
    assert_eq!(no_map.diffuse_map_report(), DiffuseMapReport::NoMap);

    let multiple = parse_mtl(Cursor::new(concat!(
        "newmtl Skin\n",
        "map_Kd tex/skin.png\n",
        "newmtl Lips\n",
        "map_Kd tex/lips.png\n",
        "newmtl Ears\n",
        "map_Kd tex/skin.png\n",
    )))
    .expect("multi-map MTL");
    assert_eq!(
        multiple.diffuse_map_report(),
        DiffuseMapReport::MultipleMaps {
            maps: vec![
                DiffuseMapBinding {
                    path: PathBuf::from("tex").join("skin.png"),
                    materials: vec!["Skin".to_owned(), "Ears".to_owned()],
                },
                DiffuseMapBinding {
                    path: PathBuf::from("tex").join("lips.png"),
                    materials: vec!["Lips".to_owned()],
                },
            ],
        }
    );
}

#[test]
fn mtl_rejects_nonlocal_or_ambiguous_diffuse_maps() {
    for map in [
        "../secret.png",
        "textures/../../secret.png",
        "C:\\textures\\face.png",
        "\\\\server\\share\\face.png",
        "//server/share/face.png",
        "https://example.invalid/face.png",
        "textures/face.png:stream",
        "NUL.png",
        "textures//face.png",
        "-s 1 1 1 textures/face.png",
    ] {
        let source = format!("newmtl Skin\nmap_Kd {map}\n");
        assert!(
            parse_mtl(Cursor::new(source)).is_err(),
            "map should be rejected: {map}"
        );
    }
}

#[test]
fn mtl_rejects_invalid_values_duplicate_state_and_properties_before_newmtl() {
    for source in [
        "Kd 1 1 1\n",
        "newmtl Skin\nKd 1 1\n",
        "newmtl Skin\nKd 1 1 NaN\n",
        "newmtl Skin\nKd 1.1 1 1\n",
        "newmtl Skin\nd -0.1\n",
        "newmtl Skin\nTr 2\n",
        "newmtl Skin\nd 1\nTr 0\n",
        "newmtl Skin\nmap_Kd tex/a.png\nmap_Kd tex/b.png\n",
        "newmtl Skin\nnewmtl Skin\n",
    ] {
        assert!(
            parse_mtl(Cursor::new(source)).is_err(),
            "MTL should be rejected: {source:?}"
        );
    }
}
