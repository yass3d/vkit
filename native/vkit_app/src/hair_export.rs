use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use vkit_core::vam::hair_writer::{HairVabDoc, encode_hair_vab};

use crate::hair_project::{HairPart, ScalpAuthoring};
use crate::hair_settings::HairSettings;

/// The caps offered when a head of hair is being built.
///
/// `PantyRegionScalp` is not among them. It is the body cap: all 66 of its
/// materials in a 1105-package library sit on pubic hair, its shell lies in a
/// thin band at v 0.132..0.261 rather than over the skull, and its bundle is
/// the one that ships no `scalp-sim3` material at all, so it has never had a
/// sheet to show. It stays in [`SCALP_PROVIDERS`] so an imported body part
/// still exports the material it came with.
pub const HEAD_SCALP_PROVIDERS: [&str; 5] = [
    "UdaneScalp",
    "KrayonScalp",
    "SoleilScalp",
    "LeytonScalp",
    "OmriScalp",
];

/// Every cap this program knows how to write a scalp material for.
pub const SCALP_PROVIDERS: [&str; 6] = [
    "UdaneScalp",
    "KrayonScalp",
    "SoleilScalp",
    "LeytonScalp",
    "PantyRegionScalp",
    "OmriScalp",
];

const SCALP_TEXTURE_DIR: &str = "textures";

const HIDDEN_SCALP_SHEET: &str = "scalp_hidden.png";

#[must_use]
pub fn scalp_is_hidden(settings: &HairSettings) -> bool {
    crate::hair_settings::param_by_key(crate::hair_settings::SCALP_OPACITY_KEY)
        .is_some_and(|param| settings.get(param) <= param.min + 1.0e-4)
}

fn transparent_sheet() -> Vec<u8> {
    let mut bytes = Vec::new();
    let sheet = image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 0]));
    let _ = image::DynamicImage::ImageRgba8(sheet).write_to(
        &mut std::io::Cursor::new(&mut bytes),
        image::ImageFormat::Png,
    );
    bytes
}

/// The keys an exported sim storable carries that no parameter owns.
///
/// Everything the parameter table owns is written by `storable_entries()`, so a
/// row here for one of those keys is a second copy that nothing reads — until
/// the key leaves the table and a number nobody chose ships. There is one
/// source for a value, and `the_sim_baseline_holds_only_what_no_parameter_owns`
/// keeps it that way.
const SIM_TABLE: [(&str, &str); 12] = [
    ("styleModeAllowControlOtherNodes", "false"),
    ("styleModeShowCurls", "false"),
    ("styleModeShowTool1", "true"),
    ("styleModeShowTool2", "true"),
    ("styleModeShowTool3", "false"),
    ("styleModeShowTool4", "false"),
    ("styleJointsSearchDistance", "0.01"),
    ("styleModeCollisionRadius", "0.004"),
    ("styleModeCollisionRadiusRoot", "0.002"),
    ("styleModeGravityMultiplier", "0"),
    ("styleModeUpHairPullStrength", "0.2"),
    ("shaderType", "Quality"),
];

fn sim_storable(uid: &str, settings: &HairSettings) -> Value {
    let mut map = Map::new();
    map.insert("id".to_owned(), json!(format!("{uid}Sim")));
    for (key, value) in SIM_TABLE {
        map.insert(key.to_owned(), json!(value));
    }
    for (key, value) in settings.storable_entries() {
        map.insert(key.to_owned(), json!(value));
    }
    map.insert("wind".to_owned(), json!(["0", "0", "0"]));
    for (key, hsv) in settings.color_entries() {
        map.insert(
            key.to_owned(),
            json!({
                "h": format!("{:.5}", hsv[0]),
                "s": format!("{:.5}", hsv[1]),
                "v": format!("{:.5}", hsv[2]),
            }),
        );
    }
    Value::Object(map)
}

fn scalp_material_storable(
    uid: &str,
    provider_name: &str,
    settings: &HairSettings,
    scalp_texture: &crate::hair_project::HairScalpTexture,
    item_name: &str,
) -> Option<Value> {
    if !SCALP_PROVIDERS.contains(&provider_name) {
        return None;
    }
    let mut map = Map::new();
    map.insert(
        "id".to_owned(),
        json!(format!("{uid}{provider_name}Material")),
    );
    for (key, value) in [
        ("renderQueue", "2423"),
        ("Specular Texture Offset", "0"),
        ("Specular Fresnel", "0.5"),
        ("Gloss Texture Offset", "0"),
        ("Diffuse Texture Offset", "0"),
        ("simTexture", ""),
    ] {
        map.insert(key.to_owned(), json!(value));
    }
    for (key, value) in settings.scalp_material_entries() {
        map.insert(key.to_owned(), json!(value));
    }
    let hides_the_cap = scalp_is_hidden(settings) && scalp_texture.alpha.is_none();
    if !scalp_texture.is_builtin() || hides_the_cap {
        for slot in 1..=4 {
            for (axis, value) in [
                ("TileX", "1"),
                ("TileY", "1"),
                ("OffsetX", "0"),
                ("OffsetY", "0"),
            ] {
                map.insert(format!("customTexture{slot}{axis}"), json!(value));
            }
        }
        for key in [
            "customTexture_MainTex",
            "customTexture_AlphaTex",
            "customTexture_SpecTex",
            "customTexture_GlossTex",
        ] {
            map.insert(key.to_owned(), json!(""));
        }
        for (slot, path) in scalp_texture.sheets() {
            let name = custom_scalp_texture_name(path, item_name, slot);
            map.insert(
                slot.vam_key().to_owned(),
                json!(format!("./{SCALP_TEXTURE_DIR}/{name}")),
            );
        }
        // Alpha Adjust is all that hides the cap, and at shader quality Low the
        // shader that owns it is disqualified for a cutout fallback with no such
        // property. A sheet with nothing in it is clipped by either of them.
        if hides_the_cap {
            map.insert(
                crate::hair_project::ScalpSlot::Alpha.vam_key().to_owned(),
                json!(format!("./{SCALP_TEXTURE_DIR}/{HIDDEN_SCALP_SHEET}")),
            );
        }
    }
    let hsv_of = |key: &str| {
        crate::hair_settings::HAIR_PARAMS
            .iter()
            .find(|param| param.key == key)
            .map(|param| settings.color_hsv(param))
            .unwrap_or([0.0; 3])
    };
    let scalp = hsv_of("Diffuse Color");
    let root = hsv_of("rootColor");
    map.insert(
        "Diffuse Color".to_owned(),
        json!({
            "h": format!("{:.5}", scalp[0]),
            "s": format!("{:.5}", scalp[1]),
            "v": format!("{:.5}", scalp[2]),
        }),
    );
    map.insert(
        "Specular Color".to_owned(),
        json!({
            "h": format!("{:.5}", root[0]),
            "s": format!("{:.5}", root[1]),
            "v": format!("{:.5}", root[2]),
        }),
    );
    map.insert(
        "Subsurface Color".to_owned(),
        json!({"h": "0", "s": "0", "v": "0"}),
    );
    Some(Value::Object(map))
}

pub fn sanitize_name(value: &str, fallback: &str) -> String {
    let cleaned: String = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || matches!(ch, ' ' | '_' | '-' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        fallback.to_owned()
    } else {
        cleaned
    }
}

pub fn authoring_guide_geometry(
    part: &HairPart,
    scalp: &ScalpAuthoring,
    joints: bool,
) -> Option<vkit_core::vam::HairGuideGeometry> {
    if part.strands.is_empty() && !part.kind.is_scalp() {
        return None;
    }
    let root_rank: std::collections::BTreeMap<u32, u32> = part
        .strands
        .keys()
        .enumerate()
        .map(|(rank, index)| (*index, rank as u32))
        .collect();
    let indices = render_triangle_indices(&root_rank, &scalp.triangles);
    let guides = part
        .strands
        .iter()
        .map(|(&scalp_index, strand)| vkit_core::vam::HairGuide {
            scalp_index,
            points_cm: strand.points_cm.clone(),
            rigidity: vec![1.0; strand.points_cm.len()],
        })
        .collect::<Vec<_>>();
    let guide_triangles = indices
        .chunks_exact(3)
        .map(|chunk| [chunk[0], chunk[1], chunk[2]])
        .collect();
    let mut root_map = vec![u32::MAX; scalp.vertices_cm.len()];
    for (&scalp_index, &rank) in &root_rank {
        if let Some(slot) = root_map.get_mut(scalp_index as usize) {
            *slot = rank;
        }
    }
    let nearby_joints = if part.style_joints && joints {
        let borrowed: Vec<&[[f32; 3]]> = part
            .strands
            .values()
            .map(|strand| strand.points_cm.as_slice())
            .collect();
        let span = part.segments.saturating_sub(1).max(1) as f32;
        vkit_core::vam::hair_joints::build_style_joints(
            &borrowed,
            vkit_core::vam::hair_joints::DEFAULT_JOINT_SEARCH_CM,
        )
        .iter()
        .flatten()
        .map(|joint| {
            let split = |flat: u32| [flat / part.segments as u32, flat % part.segments as u32];
            let a = split(joint.a);
            let b = split(joint.b);
            let elasticity = ((1.0 - a[1] as f32 / span) + (1.0 - b[1] as f32 / span)) * 0.5;
            vkit_core::vam::HairNearbyJoint { a, b, elasticity }
        })
        .collect()
    } else {
        Vec::new()
    };
    Some(vkit_core::vam::HairGuideGeometry {
        provider_name: part.provider_name.clone(),
        segments: part.segments,
        segment_length_cm: part.segment_length_cm,
        scalp_vertex_count: scalp.vertices_cm.len(),
        guides,
        guide_triangles,
        root_map,
        nearby_joints,
    })
}

pub fn authoring_scalp_material(part: &HairPart) -> vkit_core::vam::HairScalpMaterialSettings {
    let Some(storable) = scalp_material_storable(
        "preview",
        &part.provider_name,
        &part.settings,
        &part.scalp_texture,
        "preview",
    ) else {
        return vkit_core::vam::HairScalpMaterialSettings::default();
    };
    vkit_core::vam::hair_scalp_material_from_storables(std::slice::from_ref(&storable), "preview")
        .scalp_material_settings()
}

pub fn authoring_look(part: &HairPart) -> vkit_core::vam::HairLookPatch {
    vkit_core::vam::hair_look_from_storable(&sim_storable("preview", &part.settings))
}

pub fn authoring_physics(part: &HairPart) -> vkit_core::vam::HairPhysicsSettings {
    vkit_core::vam::hair_physics_from_storable(&sim_storable("preview", &part.settings)).resolve()
}

pub fn export_doc(part: &HairPart, scalp: &ScalpAuthoring) -> Result<HairVabDoc, String> {
    if part.strands.is_empty() && !part.kind.is_scalp() {
        return Err("part has no strands".to_owned());
    }
    let mut strands = std::collections::BTreeMap::new();
    for (&scalp_index, strand) in &part.strands {
        let points: Vec<[f32; 3]> = strand
            .points_cm
            .iter()
            .map(|point| {
                if scalp.export_negate_x {
                    [-point[0], point[1], point[2]]
                } else {
                    *point
                }
            })
            .collect();
        strands.insert(scalp_index, points);
    }

    let root_rank: std::collections::BTreeMap<u32, u32> = strands
        .keys()
        .enumerate()
        .map(|(rank, index)| (*index, rank as u32))
        .collect();
    let indices = render_triangle_indices(&root_rank, &scalp.triangles);

    let style_joints = if part.style_joints {
        let borrowed: Vec<&[[f32; 3]]> = strands.values().map(|points| points.as_slice()).collect();
        vkit_core::vam::hair_joints::build_style_joints(
            &borrowed,
            vkit_core::vam::hair_joints::DEFAULT_JOINT_SEARCH_CM,
        )
    } else {
        Vec::new()
    };

    Ok(HairVabDoc {
        provider_name: part.provider_name.clone(),
        segments: part.segments,
        segment_length_cm: part.segment_length_cm,
        scalp_vertex_count: scalp.vertices_cm.len(),
        strands_by_scalp_cm: strands,
        indices,
        rigidities: None,
        style_joints,
    })
}

pub struct HairExportOutcome {
    pub preset_path: PathBuf,

    /// Every file this export put on disk, so a package can gather exactly
    /// what the game would load and nothing else.
    pub files: Vec<PathBuf>,
    pub item_count: usize,
    pub triangle_count: usize,
    pub strand_count: usize,
    pub preset_thumbnails: Vec<PathBuf>,
    pub item_thumbnails: Vec<(u64, PathBuf)>,
}

fn write_hair_item(
    vam_root: &Path,
    part: &HairPart,
    scalp: &ScalpAuthoring,
    creator: &str,
    name: &str,
    sex_folder: &str,
) -> Result<HairItemOutcome, String> {
    let doc = export_doc(part, scalp)?;
    let triangle_count = doc.indices.len() / 3;
    let strand_count = doc.strands_by_scalp_cm.len();
    let vab = encode_hair_vab(&doc)?;

    let uid = format!("{creator}:{name}");
    let item_type = if sex_folder == "Male" {
        "HairMale"
    } else {
        "HairFemale"
    };
    let vam = json!({
        "itemType": item_type,
        "uid": uid,
        "displayName": name,
        "creatorName": creator,
        "tags": "",
        "isRealItem": "true",
    });
    let mut storables = vec![
        sim_storable(&uid, &part.settings),
        json!({"id": format!("{uid}ItemControl"), "disableAnatomy": "false"}),
        json!({"id": format!("{uid}Creator"), "presetName": ""}),
    ];
    if let Some(material) = scalp_material_storable(
        &uid,
        &part.provider_name,
        &part.settings,
        &part.scalp_texture,
        name,
    ) {
        storables.push(material);
    }
    let vaj = json!({"components": [], "storables": storables});

    let mut vap_storables = vec![
        sim_storable(&uid, &part.settings),
        json!({"id": format!("{uid}ItemControl"), "disableAnatomy": "false"}),
    ];
    if let Some(material) = scalp_material_storable(
        &uid,
        &part.provider_name,
        &part.settings,
        &part.scalp_texture,
        name,
    ) {
        vap_storables.push(material);
    }
    vap_storables.push(json!({"id": format!("{uid}ItemDeleter")}));
    vap_storables.push(json!({"id": format!("{uid}ItemReloader")}));
    let vap = json!({"setUnlistedParamsToDefault": "true", "storables": vap_storables});

    let item_dir = vam_root
        .join("Custom")
        .join("Hair")
        .join(sex_folder)
        .join(creator)
        .join(name);
    std::fs::create_dir_all(&item_dir)
        .map_err(|err| format!("cannot create {}: {err}", item_dir.display()))?;

    let mut files = Vec::new();
    let vab_path = item_dir.join(format!("{name}.vab"));
    std::fs::write(&vab_path, &vab)
        .map_err(|err| format!("cannot write {}: {err}", vab_path.display()))?;
    files.push(vab_path);
    if scalp_is_hidden(&part.settings) && part.scalp_texture.alpha.is_none() {
        // The cap the style does not want anyone to see.
        let textures = item_dir.join(SCALP_TEXTURE_DIR);
        std::fs::create_dir_all(&textures)
            .map_err(|err| format!("cannot create {}: {err}", textures.display()))?;
        let target = textures.join(HIDDEN_SCALP_SHEET);
        std::fs::write(&target, transparent_sheet())
            .map_err(|err| format!("cannot write {}: {err}", target.display()))?;
        files.push(target);
    }
    if !part.scalp_texture.is_builtin() {
        let textures = item_dir.join(SCALP_TEXTURE_DIR);
        std::fs::create_dir_all(&textures)
            .map_err(|err| format!("cannot create {}: {err}", textures.display()))?;
        for (slot, source) in part.scalp_texture.sheets() {
            let target = textures.join(custom_scalp_texture_name(source, name, slot));
            std::fs::copy(source, &target).map_err(|err| {
                format!(
                    "cannot copy the scalp sheet {} to {}: {err}",
                    source.display(),
                    target.display()
                )
            })?;
            files.push(target);
        }
    }
    for (suffix, document) in [
        (format!("{name}.vam"), &vam),
        (format!("{name}.vaj"), &vaj),
        (format!("{name}_Default.vap"), &vap),
    ] {
        let path = item_dir.join(suffix);
        write_json(&path, document)?;
        files.push(path);
    }

    Ok(HairItemOutcome {
        uid,
        name: name.to_owned(),
        files,
        settings: part.settings.clone(),
        scalp_texture: part.scalp_texture.clone(),
        item_id: format!("/Custom/Hair/{sex_folder}/{creator}/{name}/{name}.vam"),
        provider_name: part.provider_name.clone(),
        triangle_count,
        strand_count,
    })
}

struct HairItemOutcome {
    uid: String,
    name: String,
    files: Vec<PathBuf>,
    settings: HairSettings,
    scalp_texture: crate::hair_project::HairScalpTexture,
    item_id: String,
    provider_name: String,
    triangle_count: usize,
    strand_count: usize,
}

fn custom_scalp_texture_name(
    source: &Path,
    item_name: &str,
    slot: crate::hair_project::ScalpSlot,
) -> String {
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|value| SCALP_TEXTURE_EXTENSIONS.contains(&value.as_str()))
        .unwrap_or_else(|| "png".to_owned());
    let suffix = slot.suffix();
    format!("{}_{suffix}.{extension}", sanitize_name(item_name, "Vkit"))
}

pub const SCALP_TEXTURE_EXTENSIONS: [&str; 5] = ["jpg", "jpeg", "png", "tif", "tiff"];

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|err| format!("JSON encoding failed: {err}"))?;
    std::fs::write(path, text).map_err(|err| format!("cannot write {}: {err}", path.display()))
}

/// The patch list: the scalp mesh's own triangles, minus the incomplete ones.
///
/// `ProcessIndices` -> `FixNotCompletedPolygon` is all the game does here, and
/// it is what VaM's own create mode does. A strand in no complete triangle is
/// physics only and never drawn — it keeps its slot, its mask bit, its entry in
/// `hairRootToScalpIndices` and its points; it simply has no patch.
///
/// A synthetic triangle used to be invented for those strands out of their two
/// nearest roots. It is a complete triangle, so VaM drew `hairMultiplier`
/// children across it — between an orphan and whatever happened to be nearest,
/// which can be across a parting or over a gap — with `maxSpread` anchored on
/// the orphan and the envelope gathering toward a centre line nobody drew.
pub fn render_triangle_indices(
    root_rank: &std::collections::BTreeMap<u32, u32>,
    scalp_triangles: &[[u32; 3]],
) -> Vec<u32> {
    let mut indices = Vec::new();
    for triangle in scalp_triangles {
        if let (Some(a), Some(b), Some(c)) = (
            root_rank.get(&triangle[0]),
            root_rank.get(&triangle[1]),
            root_rank.get(&triangle[2]),
        ) {
            indices.extend([*a, *b, *c]);
        }
    }
    indices
}

pub fn export_hair_style(
    vam_root: &Path,
    parts: &[(&HairPart, &ScalpAuthoring)],
    creator: &str,
    style: &str,
    sexes: crate::hair_project::HairExportSexes,
) -> Result<HairExportOutcome, String> {
    if parts.is_empty() {
        return Err("no hair parts to export".to_owned());
    }
    let creator = sanitize_name(creator, "Vkit");
    let style = sanitize_name(style, "Vkit Hair");
    let single = parts.len() == 1;
    let both = sexes.folders().len() > 1;

    let mut first_preset: Option<PathBuf> = None;
    let mut item_count = 0;
    let mut triangle_count = 0;
    let mut strand_count = 0;
    let mut preset_thumbnails: Vec<PathBuf> = Vec::new();
    let mut item_thumbnails: Vec<(u64, PathBuf)> = Vec::new();
    let mut files: Vec<PathBuf> = Vec::new();
    for sex_folder in sexes.folders() {
        let mut items = Vec::with_capacity(parts.len());
        for (part, scalp) in parts {
            let name = if single {
                style.clone()
            } else {
                sanitize_name(&format!("{style} {}", part.name), &style)
            };
            let item = write_hair_item(vam_root, part, scalp, &creator, &name, sex_folder)?;
            files.extend(item.files.iter().cloned());
            items.push(item);
            item_thumbnails.push((
                part.id,
                vam_root
                    .join("Custom")
                    .join("Hair")
                    .join(*sex_folder)
                    .join(&creator)
                    .join(&name)
                    .join(format!("{name}.jpg")),
            ));
        }

        let hair: Vec<Value> = items
            .iter()
            .map(|item| {
                json!({
                    "id": item.item_id,
                    "internalId": item.uid,
                    "enabled": "true",
                })
            })
            .collect();
        let mut storables = vec![json!({"id": "geometry", "hair": hair})];
        for item in &items {
            storables.push(json!({"id": format!("{}Preset", item.uid), "presetName": ""}));
            storables.push(sim_storable(&item.uid, &item.settings));
            storables.push(json!({
                "id": format!("{}ItemControl", item.uid),
                "disableAnatomy": "false",
            }));
            if let Some(material) = scalp_material_storable(
                &item.uid,
                &item.provider_name,
                &item.settings,
                &item.scalp_texture,
                &item.name,
            ) {
                storables.push(material);
            }
        }
        let preset = json!({"setUnlistedParamsToDefault": "true", "storables": storables});

        let preset_dir = vam_root
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Hair")
            .join(&creator);
        std::fs::create_dir_all(&preset_dir)
            .map_err(|err| format!("cannot create {}: {err}", preset_dir.display()))?;
        let preset_name = if both && *sex_folder == "Male" {
            format!("Preset_{style} (Male).vap")
        } else {
            format!("Preset_{style}.vap")
        };
        let preset_path = preset_dir.join(preset_name);
        write_json(&preset_path, &preset)?;
        files.push(preset_path.clone());
        preset_thumbnails.push(preset_path.with_extension("jpg"));
        if first_preset.is_none() {
            first_preset = Some(preset_path);
        }
        item_count = items.len();
        triangle_count = items.iter().map(|item| item.triangle_count).sum();
        strand_count = items.iter().map(|item| item.strand_count).sum();
    }

    let preset_path = first_preset.expect("at least one sex is always chosen");
    Ok(HairExportOutcome {
        preset_path,
        files,
        item_count,
        triangle_count,
        strand_count,
        preset_thumbnails,
        item_thumbnails,
    })
}

/// Gather an installed style into a `.var` beside the game's other packages.
///
/// The files are read back from where the install put them rather than
/// rebuilt, so the package carries exactly what the game would load — the
/// custom scalp sheets among them — and picks up the thumbnails once they
/// have been rendered.
pub fn package_hair_style(
    vam_root: &Path,
    files: &[PathBuf],
    metadata: &vkit_core::vam::VarMetadata,
    existing: vkit_core::vam::ExistingPackage,
) -> Result<vkit_core::vam::VarPackage, String> {
    let mut contents = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for file in files {
        for candidate in [file.clone(), file.with_extension("jpg")] {
            if !candidate.is_file() {
                continue;
            }
            let Ok(relative) = candidate.strip_prefix(vam_root) else {
                continue;
            };
            let internal_path = relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            if !seen.insert(internal_path.clone()) {
                continue;
            }
            let bytes = std::fs::read(&candidate)
                .map_err(|err| format!("cannot read {}: {err}", candidate.display()))?;
            contents.push(vkit_core::vam::VarContent {
                internal_path,
                bytes,
            });
        }
    }
    if contents.is_empty() {
        return Err("nothing to package: install the style first".to_owned());
    }
    let directory = vam_root.join("AddonPackages");
    std::fs::create_dir_all(&directory)
        .map_err(|err| format!("cannot create {}: {err}", directory.display()))?;
    vkit_core::vam::write_var_package(&directory, metadata, &contents, existing)
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hair_project::build_scalp_authoring;
    use vkit_core::vam::{BuiltinHairScalp, HairScalpGeometry};

    #[test]
    fn a_package_carries_the_whole_install_including_the_sheets_it_needs() {
        let dir = tempfile::tempdir().expect("temp");
        let sheet = dir.path().join("my scalp.png");
        std::fs::write(&sheet, b"a scalp sheet").expect("sheet");

        let scalp = build_scalp_authoring(&test_scalp()).expect("scalp");
        let mut project = crate::hair_project::HairProject::default();
        let id = project.add_part("UdaneScalp");
        let part = project.parts.iter_mut().find(|p| p.id == id).unwrap();
        part.name = "Bob".to_owned();
        part.scalp_texture = crate::hair_project::HairScalpTexture {
            diffuse: None,
            alpha: Some(sheet),
        };
        part.plant(&scalp, &[0, 1, 4]);
        let parts: Vec<_> = project.parts.iter().map(|part| (part, &scalp)).collect();
        let outcome = export_hair_style(
            dir.path(),
            &parts,
            "Vkit",
            "Bob",
            crate::hair_project::HairExportSexes::Female,
        )
        .expect("export");

        let package = package_hair_style(
            dir.path(),
            &outcome.files,
            &vkit_core::vam::VarMetadata {
                creator: "Vkit".to_owned(),
                package: "Bob".to_owned(),
                ..vkit_core::vam::VarMetadata::default()
            },
            vkit_core::vam::ExistingPackage::Keep,
        )
        .expect("package");

        assert!(package.path.is_file(), "the package must reach the disk");
        assert!(
            package
                .contents
                .iter()
                .any(|entry| entry.ends_with("Bob.vab")),
            "the geometry has to be in there: {:?}",
            package.contents
        );
        assert!(
            package
                .contents
                .iter()
                .any(|entry| entry.contains("/textures/") && entry.ends_with(".png")),
            "a custom scalp sheet is the whole reason to package rather than              hand someone a preset: {:?}",
            package.contents
        );
        assert!(
            package
                .contents
                .iter()
                .all(|entry| !entry.contains(std::path::MAIN_SEPARATOR)),
            "a package addresses its files with forward slashes: {:?}",
            package.contents
        );
    }

    #[test]
    fn a_both_sexes_export_files_the_style_into_each_registry() {
        let dir = tempfile::tempdir().expect("temp");
        let scalp = build_scalp_authoring(&test_scalp()).expect("scalp");
        let mut project = crate::hair_project::HairProject::default();
        let id = project.add_part("UdaneScalp");
        let part = project.parts.iter_mut().find(|p| p.id == id).unwrap();
        part.name = "Unisex".to_owned();
        part.plant(&scalp, &[0, 1, 4]);
        let parts: Vec<_> = project.parts.iter().map(|part| (part, &scalp)).collect();
        let outcome = export_hair_style(
            dir.path(),
            &parts,
            "Vkit",
            "Unisex",
            crate::hair_project::HairExportSexes::Both,
        )
        .expect("export");

        for (folder, item_type) in [("Female", "HairFemale"), ("Male", "HairMale")] {
            let item = dir
                .path()
                .join("Custom")
                .join("Hair")
                .join(folder)
                .join("Vkit")
                .join("Unisex")
                .join("Unisex.vam");
            let text = std::fs::read_to_string(&item).expect("item written");
            assert!(text.contains(item_type), "{folder} carries {item_type}");
            assert!(item.with_extension("vab").exists(), "{folder} geometry");
        }
        let presets = dir
            .path()
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Hair")
            .join("Vkit");
        assert!(presets.join("Preset_Unisex.vap").exists());
        let male =
            std::fs::read_to_string(presets.join("Preset_Unisex (Male).vap")).expect("male preset");
        assert!(
            male.contains("/Custom/Hair/Male/Vkit/Unisex/Unisex.vam"),
            "the male preset wears the male item",
        );
        assert_eq!(outcome.preset_thumbnails.len(), 2);
        assert_eq!(outcome.item_thumbnails.len(), 2, "one slot per registry");
    }

    #[test]
    fn a_multi_part_style_ships_as_items_plus_a_preset_that_lists_them() {
        let dir = tempfile::tempdir().expect("temp");
        let scalp = build_scalp_authoring(&test_scalp()).expect("scalp");
        let mut project = crate::hair_project::HairProject::default();
        let mut parts = Vec::new();
        for label in ["base", "bangs"] {
            let id = project.add_part("UdaneScalp");
            let part = project.parts.iter_mut().find(|p| p.id == id).unwrap();
            part.name = label.to_owned();
            part.plant(&scalp, &[0, 1, 4]);
            parts.push(id);
        }
        let owned: Vec<_> = project.parts.iter().map(|part| (part, &scalp)).collect();
        let outcome = export_hair_style(
            dir.path(),
            &owned,
            "Vkit",
            "Style",
            crate::hair_project::HairExportSexes::Female,
        )
        .expect("export");
        assert_eq!(outcome.item_count, 2);

        for name in ["Style base", "Style bangs"] {
            let item = dir.path().join("Custom/Hair/Female/Vkit").join(name);
            for ext in ["vab", "vam", "vaj"] {
                assert!(
                    item.join(format!("{name}.{ext}")).is_file(),
                    "missing {name}.{ext}",
                );
            }
        }

        let preset: Value =
            serde_json::from_str(&std::fs::read_to_string(&outcome.preset_path).expect("preset"))
                .expect("json");
        let geometry = preset["storables"]
            .as_array()
            .unwrap()
            .iter()
            .find(|storable| storable["id"] == "geometry")
            .expect("geometry storable");
        let hair = geometry["hair"].as_array().expect("hair array");
        assert_eq!(hair.len(), 2, "the preset must list every part");
        for (entry, name) in hair.iter().zip(["Style base", "Style bangs"]) {
            assert_eq!(entry["internalId"], format!("Vkit:{name}"));
            assert_eq!(
                entry["id"],
                format!("/Custom/Hair/Female/Vkit/{name}/{name}.vam")
            );
            assert_eq!(entry["enabled"], "true");
        }
        for name in ["Style base", "Style bangs"] {
            assert!(
                preset["storables"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|storable| storable["id"] == format!("Vkit:{name}Sim")),
                "no Sim storable for {name}",
            );
        }
    }

    #[test]
    fn the_density_and_multiplier_sliders_reach_the_file() {
        let mut settings = HairSettings::default();
        for (key, value) in [("curveDensity", 2.0), ("hairMultiplier", 5.0)] {
            let param = crate::hair_settings::HAIR_PARAMS
                .iter()
                .find(|param| param.key == key)
                .expect("param");
            settings.set(param, value);
        }
        let sim = sim_storable("T:Test", &settings);
        assert_eq!(sim["curveDensity"], "2");
        assert_eq!(sim["hairMultiplier"], "5");
    }

    pub(super) fn test_scalp() -> BuiltinHairScalp {
        BuiltinHairScalp {
            provider_name: "UdaneScalp".to_owned(),
            geometry: HairScalpGeometry {
                materials: Vec::new(),
                vertices_cm: vec![
                    [-2.0, 10.0, -1.0],
                    [2.0, 10.0, -1.0],
                    [-2.0, 10.0, 1.0],
                    [2.0, 10.0, 1.0],
                    [0.0, 10.5, 0.0],
                ],
                uvs: vec![[0.0, 0.0]; 5],
                triangles: vec![[0, 1, 4], [1, 3, 4], [3, 2, 4], [2, 0, 4]],
            },
        }
    }

    /// The body cap is not a head cap. All 66 `PantyRegionScalpMaterial`
    /// storables in a 1105-package library sit on pubic hair, its UV shell is a
    /// band at v 0.132..0.261 rather than a skull, and `p_gen_mat` is the one
    /// scalp bundle shipping no `scalp-sim3` material — so it has never had a
    /// sheet to show in a head-hair picker.
    #[test]
    fn the_body_cap_is_not_offered_for_a_head_but_still_exports() {
        assert!(!HEAD_SCALP_PROVIDERS.contains(&"PantyRegionScalp"));
        assert!(SCALP_PROVIDERS.contains(&"PantyRegionScalp"));
        for provider in HEAD_SCALP_PROVIDERS {
            assert!(
                SCALP_PROVIDERS.contains(&provider),
                "{provider} can be chosen but has no material writer",
            );
        }
        let material = scalp_material_storable(
            "Vkit:Bush",
            "PantyRegionScalp",
            &HairSettings::default(),
            &crate::hair_project::HairScalpTexture::default(),
            "Bush",
        );
        assert!(
            material.is_some(),
            "an imported body part still exports the material it came with",
        );
    }

    /// `SIM_TABLE` is the baseline every exported sim storable starts from, and
    /// `storable_entries()` then overwrites every key the parameter table owns.
    /// A row that disagrees with the table is therefore invisible — until a key
    /// leaves `HAIR_PARAMS`, at which point a number nobody chose ships. Two
    /// sources for one value is the shape the ledger keeps retiring; this holds
    /// them equal instead.
    /// The IBL filter is per mesh, not one number for all of them: across 1125
    /// shipped scalp materials Udane is 0.5 in 303 of 304, PantyRegion 0.0 in
    /// 136 of 136, and Krayon, Leyton and Soleil 0.7. The exporter used to
    /// write a flat 0.7 while the right numbers sat unread in `SCALP_STOCK`.
    #[test]
    fn each_cap_exports_the_ambient_filter_its_own_mesh_ships() {
        for (provider, wanted) in [
            ("UdaneScalp", "0.5"),
            ("PantyRegionScalp", "0"),
            ("KrayonScalp", "0.7"),
            ("LeytonScalp", "0.7"),
            ("SoleilScalp", "0.7"),
        ] {
            let mut settings = HairSettings::default();
            settings.wear(provider);
            let material = scalp_material_storable(
                "Vkit:Bob",
                provider,
                &settings,
                &crate::hair_project::HairScalpTexture::default(),
                "Bob",
            )
            .expect("a material");
            assert_eq!(
                material["Global Illumination Filter"], wanted,
                "{provider} ships {wanted}",
            );
        }
    }

    #[test]
    fn the_sim_baseline_holds_only_what_no_parameter_owns() {
        let defaults = crate::hair_settings::HairSettings::default().storable_entries();
        let table: std::collections::BTreeMap<&str, String> = defaults.into_iter().collect();
        let disagreed: Vec<&str> = SIM_TABLE
            .iter()
            .map(|(key, _)| *key)
            .filter(|key| table.contains_key(key))
            .collect();
        assert!(
            disagreed.is_empty(),
            "{} sim keys are declared twice: {disagreed:#?}",
            disagreed.len(),
        );
    }

    #[test]
    fn sim_table_pins_the_long_dynamic_sentinels() {
        let sim = sim_storable("T:Test", &HairSettings::default());
        assert_eq!(sim["mainRigidity"], "0.01");
        assert_eq!(sim["weight"], "1.5");
        assert_eq!(sim["hairMultiplier"], "16");
        assert_eq!(sim["curveDensity"], "24");
        assert_eq!(sim["maxSpread"], "0.025");
        assert_eq!(sim["wind"], json!(["0", "0", "0"]));
        assert_eq!(sim["usePaintedRigidity"], "false");
        assert_eq!(sim["id"], "T:TestSim");
    }

    /// `ProcessIndices` -> `FixNotCompletedPolygon`: a triangle whose three
    /// corners are not all planted is dropped, and the strands on it are
    /// physics only. Nothing is invented to keep them drawable.
    #[test]
    fn a_strand_in_no_complete_triangle_keeps_its_slot_and_gets_no_patch() {
        use crate::hair_project::{HairProject, build_scalp_authoring};
        use vkit_core::vam::{BuiltinHairScalp, HairScalpGeometry};

        let vertices_cm: Vec<[f32; 3]> = (0..6)
            .map(|index| [index as f32, 10.0, if index % 2 == 0 { 0.0 } else { 1.0 }])
            .collect();
        let scalp = BuiltinHairScalp {
            provider_name: "UdaneScalp".to_owned(),
            geometry: HairScalpGeometry {
                materials: Vec::new(),
                uvs: vec![[0.0, 0.0]; vertices_cm.len()],
                triangles: vec![[0, 1, 2], [1, 3, 2], [2, 3, 4], [3, 5, 4]],
                vertices_cm,
            },
        };
        let authoring = build_scalp_authoring(&scalp).expect("build");
        let mut project = HairProject::default();
        let id = project.add_part("UdaneScalp");
        let part = project.parts.iter_mut().find(|p| p.id == id).unwrap();
        // Every other vertex: no scalp triangle has all three corners planted.
        part.plant(&authoring, &[0, 2, 4]);

        let doc = export_doc(part, &authoring).expect("doc");
        assert!(
            doc.indices.is_empty(),
            "a patch was invented for strands the game leaves physics-only: {:?}",
            doc.indices,
        );
        assert_eq!(
            doc.strands_by_scalp_cm.len(),
            3,
            "the strands themselves keep their slots and their physics",
        );
        let encoded = encode_hair_vab(&doc).expect("encode");
        vkit_core::vam::parse_hair_vab(&encoded, "test").expect("parse");
    }

    #[test]
    fn a_fully_planted_patch_is_the_scalp_triangle_it_came_from() {
        use crate::hair_project::{HairProject, build_scalp_authoring};
        use vkit_core::vam::{BuiltinHairScalp, HairScalpGeometry};

        let scalp = BuiltinHairScalp {
            provider_name: "UdaneScalp".to_owned(),
            geometry: HairScalpGeometry {
                materials: Vec::new(),
                vertices_cm: vec![
                    [-2.0, 10.0, -1.0],
                    [2.0, 10.0, -1.0],
                    [-2.0, 10.0, 1.0],
                    [2.0, 10.0, 1.0],
                ],
                uvs: vec![[0.0, 0.0]; 4],
                triangles: vec![[0, 1, 2], [1, 3, 2]],
            },
        };
        let authoring = build_scalp_authoring(&scalp).expect("build");
        let mut project = HairProject::default();
        let id = project.add_part("UdaneScalp");
        let part = project.parts.iter_mut().find(|p| p.id == id).unwrap();
        part.plant(&authoring, &[0, 1, 2]);

        let doc = export_doc(part, &authoring).expect("doc");
        assert_eq!(doc.indices.len(), 3, "one complete triangle, one patch");
        let mut corners = doc.indices.clone();
        corners.sort_unstable();
        assert_eq!(corners, vec![0, 1, 2]);

        // A lone strand has no complete triangle and so has no patch at all.
        let part = project.parts.iter_mut().find(|p| p.id == id).unwrap();
        part.strands.retain(|index, _| *index == 0);
        let doc = export_doc(part, &authoring).expect("doc");
        assert!(doc.indices.is_empty());
    }

    #[test]
    fn export_negates_positions_and_leaves_root_indices_alone() {
        use crate::hair_project::{HairProject, build_scalp_authoring};
        use vkit_core::vam::{BuiltinHairScalp, HairScalpGeometry};

        let scalp = BuiltinHairScalp {
            provider_name: "UdaneScalp".to_owned(),
            geometry: HairScalpGeometry {
                materials: Vec::new(),
                vertices_cm: vec![
                    [-2.0, 10.0, -1.0],
                    [2.0, 10.0, -1.0],
                    [-2.0, 10.0, 1.0],
                    [2.0, 10.0, 1.0],
                    [0.0, 10.5, 0.0],
                ],
                uvs: vec![[0.0, 0.0]; 5],
                triangles: vec![[0, 1, 4], [1, 3, 4], [3, 2, 4], [2, 0, 4]],
            },
        };
        let authoring = build_scalp_authoring(&scalp).expect("build");
        assert!(authoring.export_negate_x);
        assert_eq!(authoring.mirror_pair[0], 1);
        assert_eq!(authoring.mirror_pair[1], 0);
        assert_eq!(authoring.mirror_pair[4], 4);

        let mut project = HairProject::default();
        let id = project.add_part("UdaneScalp");
        let part = project.parts.iter_mut().find(|p| p.id == id).unwrap();
        part.plant(&authoring, &[0, 1, 4]);
        let authored_root_of_0 = part.strands[&0].points_cm[0];

        let doc = export_doc(part, &authoring).expect("doc");
        let exported: Vec<u32> = doc.strands_by_scalp_cm.keys().copied().collect();
        assert_eq!(exported, vec![0, 1, 4]);
        let root = &doc.strands_by_scalp_cm[&0][0];
        assert_eq!(root[0], -authored_root_of_0[0]);
        assert_eq!(root[1], authored_root_of_0[1]);
        assert_eq!(root[2], authored_root_of_0[2]);
        assert_eq!(*root, scalp.geometry.vertices_cm[0]);
        let encoded = encode_hair_vab(&doc).expect("encode");
        let parsed = vkit_core::vam::parse_hair_vab(&encoded, "test").expect("parse");
        assert_eq!(parsed.root_map, vec![0, 1, 4]);
        assert!(!parsed.guide_triangles.is_empty());
    }
}

#[cfg(test)]
mod settings_export_tests {
    use super::*;
    use crate::hair_project::build_scalp_authoring;
    use crate::hair_settings::{HAIR_PARAMS, HairSettings};

    #[test]
    fn a_parts_overrides_land_in_its_sim_storable() {
        let multiplier = HAIR_PARAMS
            .iter()
            .find(|param| param.key == "hairMultiplier")
            .unwrap();
        let width = HAIR_PARAMS
            .iter()
            .find(|param| param.key == "width")
            .unwrap();
        let sim_off = HAIR_PARAMS
            .iter()
            .find(|param| param.key == "simulationEnabled")
            .unwrap();

        let mut settings = HairSettings::default();
        settings.set(multiplier, 48.0);
        settings.set(width, 0.0004);
        settings.set(sim_off, 0.0);

        let sim = sim_storable("Vkit:Test", &settings);
        assert_eq!(sim["hairMultiplier"], "48");
        assert_eq!(sim["width"], "0.0004");
        assert_eq!(sim["simulationEnabled"], "false");
        assert_eq!(sim["weight"], "1.5");
        assert_eq!(sim["id"], "Vkit:TestSim");
    }

    #[test]
    fn every_panel_parameter_reaches_the_storable_it_targets() {
        use crate::hair_settings::HairParamTarget;

        let settings = HairSettings::default();
        let sim = sim_storable("Vkit:Test", &settings);
        let scalp = scalp_material_storable(
            "Vkit:Test",
            "UdaneScalp",
            &settings,
            &crate::hair_project::HairScalpTexture::default(),
            "Test",
        )
        .expect("a head provider has a scalp material");
        for param in &HAIR_PARAMS {
            let (block, name) = match param.target {
                HairParamTarget::Sim => (&sim, "Sim"),
                HairParamTarget::ScalpMaterial => (&scalp, "scalp material"),
            };
            assert!(
                block.get(param.key).is_some(),
                "{} is offered in the panel but never written to the {name} storable",
                param.key,
            );
        }
    }

    #[test]
    fn a_custom_scalp_mask_is_copied_beside_the_item_and_referenced_relatively() {
        use crate::hair_project::HairScalpTexture;

        let dir = tempfile::tempdir().expect("temp");
        let source = dir.path().join("my hairline.PNG");
        std::fs::write(&source, b"not really a png").expect("write mask");

        let settings = HairSettings::default();
        let texture = HairScalpTexture {
            diffuse: Some(source.clone()),
            alpha: None,
        };
        let scalp =
            scalp_material_storable("Vkit:Bob", "UdaneScalp", &settings, &texture, "Bob").unwrap();
        assert_eq!(scalp["customTexture_MainTex"], "./textures/Bob_scalp.png");
        for key in ["customTexture_SpecTex", "customTexture_GlossTex"] {
            assert_eq!(scalp[key], "", "{key} should be empty");
        }
        assert_eq!(
            scalp["customTexture_AlphaTex"],
            format!("./{SCALP_TEXTURE_DIR}/{HIDDEN_SCALP_SHEET}"),
            "these settings still hide the cap, and hiding has to survive the fallback",
        );
        assert_eq!(scalp["customTexture4TileX"], "1");

        let mut shown = HairSettings::default();
        let opacity = crate::hair_settings::param_by_key(crate::hair_settings::SCALP_OPACITY_KEY)
            .expect("the cap has an opacity");
        shown.set(opacity, opacity.max);
        let seen =
            scalp_material_storable("Vkit:Bob", "UdaneScalp", &shown, &texture, "Bob").unwrap();
        assert_eq!(
            seen["customTexture_AlphaTex"], "",
            "raise the opacity and the erasing sheet goes away",
        );

        let built_in = scalp_material_storable(
            "Vkit:Bob",
            "UdaneScalp",
            &settings,
            &HairScalpTexture::default(),
            "Bob",
        )
        .unwrap();
        assert_eq!(
            built_in["customTexture_AlphaTex"],
            format!("./{SCALP_TEXTURE_DIR}/{HIDDEN_SCALP_SHEET}"),
            "a built-in cap that is hidden still needs the sheet that clips it",
        );

        let built_in_shown = scalp_material_storable(
            "Vkit:Bob",
            "UdaneScalp",
            &shown,
            &HairScalpTexture::default(),
            "Bob",
        )
        .unwrap();
        assert!(
            built_in_shown
                .as_object()
                .unwrap()
                .keys()
                .all(|key| !key.starts_with("customTexture")),
            "a built-in cap meant to be seen says nothing about textures",
        );

        let cutout = dir.path().join("hairline mask.jpg");
        std::fs::write(&cutout, b"not really a jpeg").expect("write cutout");
        let masked = HairScalpTexture {
            diffuse: None,
            alpha: Some(cutout.clone()),
        };
        let alpha_only =
            scalp_material_storable("Vkit:Bob", "UdaneScalp", &settings, &masked, "Bob").unwrap();
        assert_eq!(
            alpha_only["customTexture_AlphaTex"], "./textures/Bob_scalp_alpha.jpg",
            "the cutout is the slot nearly every VaM hair uses, and it must survive export",
        );
        assert_eq!(
            alpha_only["customTexture_MainTex"], "",
            "a cutout must not be handed to VaM as a colour sheet",
        );

        let both = HairScalpTexture {
            diffuse: Some(source.clone()),
            alpha: Some(cutout.clone()),
        };
        let paired =
            scalp_material_storable("Vkit:Bob", "UdaneScalp", &settings, &both, "Bob").unwrap();
        assert_eq!(paired["customTexture_MainTex"], "./textures/Bob_scalp.png");
        assert_eq!(
            paired["customTexture_AlphaTex"], "./textures/Bob_scalp_alpha.jpg",
            "the two sheets keep separate names so neither overwrites the other",
        );

        let scalp_authoring = build_scalp_authoring(&super::tests::test_scalp()).expect("scalp");
        let mut project = crate::hair_project::HairProject::default();
        let id = project.add_part("UdaneScalp");
        let part = project.parts.iter_mut().find(|p| p.id == id).unwrap();
        part.plant(&scalp_authoring, &[0, 1, 4]);
        part.scalp_texture = texture;
        let root = dir.path().join("VaM");
        let owned = vec![(&*part, &scalp_authoring)];
        export_hair_style(
            &root,
            &owned,
            "Vkit",
            "Bob",
            crate::hair_project::HairExportSexes::Female,
        )
        .expect("export");
        assert!(
            root.join("Custom/Hair/Female/Vkit/Bob/textures/Bob_scalp.png")
                .is_file(),
            "the sheet was not copied beside the item",
        );
    }

    #[test]
    fn the_scalp_cap_defaults_to_hidden_and_follows_the_hair_colour() {
        let settings = HairSettings::default();
        let scalp = scalp_material_storable(
            "Vkit:Test",
            "UdaneScalp",
            &settings,
            &crate::hair_project::HairScalpTexture::default(),
            "Test",
        )
        .expect("scalp material");
        assert_eq!(scalp["Alpha Adjust"], "-1");
        assert_eq!(scalp["Specular Intensity"], "0");
        assert_eq!(scalp["Specular Fresnel"], "0.5");
        assert_eq!(scalp["Diffuse Color"]["s"], "0.00000");
        assert_eq!(scalp["Diffuse Color"]["v"], "1.00000");

        let root = HAIR_PARAMS
            .iter()
            .find(|param| param.key == "rootColor")
            .unwrap();
        let mut tinted = HairSettings::default();
        tinted.set_color_channel(root, 0, 255.0);
        tinted.set_color_channel(root, 1, 0.0);
        tinted.set_color_channel(root, 2, 0.0);
        let scalp = scalp_material_storable(
            "Vkit:Test",
            "UdaneScalp",
            &tinted,
            &crate::hair_project::HairScalpTexture::default(),
            "Test",
        )
        .expect("material");
        assert_eq!(scalp["Diffuse Color"]["s"], "0.00000");
        assert_eq!(scalp["Diffuse Color"]["v"], "1.00000");
        assert_eq!(scalp["Specular Color"]["s"], "1.00000");
        assert_eq!(scalp["Specular Color"]["v"], "1.00000");
    }

    #[test]
    fn a_hidden_cap_carries_a_sheet_that_clips_it_at_every_shader_quality() {
        let hidden = HairSettings::default();
        assert!(
            scalp_is_hidden(&hidden),
            "the default cap is meant to vanish"
        );
        let material = scalp_material_storable(
            "Vkit:Test",
            "UdaneScalp",
            &hidden,
            &crate::hair_project::HairScalpTexture::default(),
            "Test",
        )
        .expect("scalp material");
        assert_eq!(
            material["customTexture_AlphaTex"],
            format!("./{SCALP_TEXTURE_DIR}/{HIDDEN_SCALP_SHEET}"),
            "Alpha Adjust alone stops hiding it the moment the shader falls back",
        );
        assert_eq!(
            material["customTexture1TileX"], "1",
            "the siblings come too"
        );

        let mut shown = HairSettings::default();
        let opacity = crate::hair_settings::param_by_key(crate::hair_settings::SCALP_OPACITY_KEY)
            .expect("the cap has an opacity");
        shown.set(opacity, opacity.max);
        assert!(!scalp_is_hidden(&shown));
        let material = scalp_material_storable(
            "Vkit:Test",
            "UdaneScalp",
            &shown,
            &crate::hair_project::HairScalpTexture::default(),
            "Test",
        )
        .expect("scalp material");
        assert!(
            material.get("customTexture_AlphaTex").is_none(),
            "a cap meant to be seen is not handed a sheet that erases it",
        );
    }

    #[test]
    fn the_transparent_sheet_is_a_png_with_nothing_in_it() {
        let bytes = transparent_sheet();
        assert_eq!(
            &bytes[..8],
            &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a],
            "it has to be a PNG",
        );
        let decoded = image::load_from_memory(&bytes)
            .expect("it decodes")
            .to_rgba8();
        assert!(
            decoded.pixels().all(|pixel| pixel.0[3] == 0),
            "every pixel has to fall under any cutoff the fallback picks",
        );
    }

    #[test]
    fn the_guides_come_out_in_scalp_order_carrying_the_points_they_were_planted_with() {
        use crate::hair_project::{HairProject, build_scalp_authoring};
        use vkit_core::vam::{BuiltinHairScalp, HairScalpGeometry};

        let vertices_cm: Vec<[f32; 3]> = (0..6)
            .map(|index| [index as f32, 10.0, if index % 2 == 0 { 0.0 } else { 1.0 }])
            .collect();
        let scalp = BuiltinHairScalp {
            provider_name: "UdaneScalp".to_owned(),
            geometry: HairScalpGeometry {
                materials: Vec::new(),
                uvs: vec![[0.0, 0.0]; vertices_cm.len()],
                triangles: vec![[0, 1, 2], [1, 3, 2], [2, 3, 4], [3, 5, 4]],
                vertices_cm,
            },
        };
        let authoring = build_scalp_authoring(&scalp).expect("build");
        let mut project = HairProject::default();
        let id = project.add_part("UdaneScalp");
        let part = project.parts.iter_mut().find(|p| p.id == id).unwrap();
        part.plant(&authoring, &[4, 0, 2]);

        let geometry = authoring_guide_geometry(part, &authoring, true).expect("geometry");

        let planted: Vec<u32> = part.strands.keys().copied().collect();
        let emitted: Vec<u32> = geometry
            .guides
            .iter()
            .map(|guide| guide.scalp_index)
            .collect();
        assert_eq!(
            emitted, planted,
            "the guides must follow the strand map's own order -- the render              triangles are indexed by rank in exactly that order"
        );

        for guide in &geometry.guides {
            let strand = &part.strands[&guide.scalp_index];
            assert_eq!(
                guide.points_cm, strand.points_cm,
                "guide {} was handed points that are not its strand's",
                guide.scalp_index
            );
            assert_eq!(
                guide.rigidity.len(),
                guide.points_cm.len(),
                "every point needs its own rigidity"
            );
        }

        for (rank, index) in planted.iter().enumerate() {
            assert_eq!(
                geometry.root_map[*index as usize], rank as u32,
                "the root map must send a scalp vertex to the rank the guides use"
            );
        }
    }

    #[test]
    fn the_preview_may_skip_the_joint_graph_but_the_export_never_does() {
        use crate::hair_project::{HairProject, build_scalp_authoring};
        use vkit_core::vam::{BuiltinHairScalp, HairScalpGeometry};

        let vertices_cm: Vec<[f32; 3]> = (0..6)
            .map(|index| {
                [
                    index as f32 * 0.4,
                    10.0,
                    if index % 2 == 0 { 0.0 } else { 0.4 },
                ]
            })
            .collect();
        let scalp = BuiltinHairScalp {
            provider_name: "UdaneScalp".to_owned(),
            geometry: HairScalpGeometry {
                materials: Vec::new(),
                uvs: vec![[0.0, 0.0]; vertices_cm.len()],
                triangles: vec![[0, 1, 2], [1, 3, 2], [2, 3, 4], [3, 5, 4]],
                vertices_cm,
            },
        };
        let authoring = build_scalp_authoring(&scalp).expect("build");
        let mut project = HairProject::default();
        let id = project.add_part("UdaneScalp");
        let part = project.parts.iter_mut().find(|p| p.id == id).unwrap();
        part.plant(&authoring, &[0, 1, 2, 3, 4]);
        part.style_joints = true;

        let with_joints = authoring_guide_geometry(part, &authoring, true).expect("geometry");
        assert!(
            !with_joints.nearby_joints.is_empty(),
            "a part that asks for style joints must get them when they are wanted"
        );

        let without = authoring_guide_geometry(part, &authoring, false).expect("geometry");
        assert!(
            without.nearby_joints.is_empty(),
            "and must not pay for them when nothing will solve them"
        );
        assert_eq!(
            without.guides.len(),
            with_joints.guides.len(),
            "skipping joints must not change the hair itself"
        );

        assert!(
            !authoring_guide_geometry(part, &authoring, true)
                .expect("geometry")
                .nearby_joints
                .is_empty(),
            "the joint-bearing path is still the default, whatever the viewport skipped"
        );
    }
}
