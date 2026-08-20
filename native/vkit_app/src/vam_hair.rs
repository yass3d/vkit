pub fn part_storables(part: &vkit_core::vam::HairPartReference) -> Option<Vec<serde_json::Value>> {
    part.appearance
        .as_ref()
        .and_then(|locator| vkit_core::vam::read_hair_storables(locator).ok())
}

pub fn preset_storables(preset: &vkit_core::vam::HairPreset) -> Option<Vec<serde_json::Value>> {
    vkit_core::vam::read_hair_storables(&preset.source).ok()
}

fn scalp_sheet_path(
    part: &vkit_core::vam::HairPartReference,
    reference: Option<&str>,
) -> Option<std::path::PathBuf> {
    let reference = reference
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("NULL"))?;
    let extension = reference
        .rsplit_once('.')
        .map(|(_, tail)| tail.trim().to_ascii_lowercase())
        .filter(|tail| crate::hair_export::SCALP_TEXTURE_EXTENSIONS.contains(&tail.as_str()))?;

    let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
    sha2::Digest::update(&mut hasher, part.geometry.display_key().as_bytes());
    sha2::Digest::update(&mut hasher, b"\0");
    sha2::Digest::update(&mut hasher, reference.as_bytes());
    let stem = crate::cache_paths::hex_prefix(&sha2::Digest::finalize(hasher), 12);

    Some(
        vkit_core::cache_root()?
            .join("hair-scalp-sheets")
            .join(format!("{stem}.{extension}")),
    )
}

fn store_scalp_sheet(
    path: Option<std::path::PathBuf>,
    bytes: Option<Vec<u8>>,
) -> Option<std::path::PathBuf> {
    let path = path?;
    if path.is_file() {
        return Some(path);
    }
    let bytes = bytes?;
    std::fs::create_dir_all(path.parent()?).ok()?;
    std::fs::write(&path, bytes).ok()?;
    Some(path)
}

pub fn extract_scalp_sheets(
    part: &vkit_core::vam::HairPartReference,
    look: &vkit_core::vam::HairLookPatch,
) -> crate::hair_project::HairScalpTexture {
    let diffuse = scalp_sheet_path(part, look.scalp_diffuse.as_deref());
    let alpha = scalp_sheet_path(part, look.scalp_alpha.as_deref());
    let missing = [diffuse.as_ref(), alpha.as_ref()]
        .into_iter()
        .flatten()
        .any(|path| !path.is_file());
    let sheets = if missing {
        vkit_core::vam::load_hair_scalp_textures(part, look)
    } else {
        vkit_core::vam::HairScalpTextureBytes::default()
    };
    crate::hair_project::HairScalpTexture {
        diffuse: store_scalp_sheet(diffuse, sheets.diffuse),
        alpha: store_scalp_sheet(alpha, sheets.alpha),
    }
}
