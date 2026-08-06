use sha2::{Digest, Sha256};
use std::{collections::HashMap, fs, path::Path};

const ICON_SCHEMA: &str = "vkit-icon-v1";
const ICON_SIZES: &str = "16,20,24,32,40,48,64,128,256";
const ICON_RESAMPLE: &str = "rgba-lanczos";

fn sha256(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn parse_icon_manifest(path: &Path) -> Result<HashMap<String, String>, String> {
    let contents =
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut values = HashMap::new();
    for (index, raw_line) in contents.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!(
                "{}:{} is not a key=value entry",
                path.display(),
                index + 1
            ));
        };
        values.insert(key.trim().to_owned(), value.trim().to_owned());
    }
    Ok(values)
}

fn verify_icon_assets(manifest_dir: &Path) -> Result<(), String> {
    let project_root = manifest_dir.join("../..");
    let source_path = project_root.join("design/logo.png");
    let icon_path = manifest_dir.join("resources/vkit.ico");
    let hash_path = manifest_dir.join("resources/vkit-icon.sha256");
    let manifest = parse_icon_manifest(&hash_path)?;

    for (key, expected) in [
        ("schema", ICON_SCHEMA),
        ("source", "design/logo.png"),
        ("sizes", ICON_SIZES),
        ("resample", ICON_RESAMPLE),
    ] {
        if manifest.get(key).map(String::as_str) != Some(expected) {
            return Err(format!(
                "{} has invalid {key}; expected {expected:?}",
                hash_path.display()
            ));
        }
    }

    for (key, path) in [
        ("source_sha256", source_path.as_path()),
        ("ico_sha256", icon_path.as_path()),
    ] {
        let expected = manifest
            .get(key)
            .ok_or_else(|| format!("{} is missing {key}", hash_path.display()))?;
        let actual = sha256(path)?;
        if actual != *expected {
            return Err(format!(
                "{} is stale (SHA-256 {actual}, expected {expected})",
                path.display()
            ));
        }
    }
    Ok(())
}

fn main() {
    println!("cargo:rerun-if-changed=resources/windows.rc");
    println!("cargo:rerun-if-changed=resources/vkit.manifest");
    println!("cargo:rerun-if-changed=resources/vkit.ico");
    println!("cargo:rerun-if-changed=resources/vkit-icon.sha256");
    println!("cargo:rerun-if-changed=../../design/logo.png");
    println!("cargo:rerun-if-changed=../../build/windows/Generate-Icon.py");
    println!("cargo:rerun-if-changed=../../build/windows/THIRD-PARTY-NOTICE.txt");

    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .expect("CARGO_MANIFEST_DIR is unavailable");
    if let Err(error) = verify_icon_assets(&manifest_dir) {
        panic!(
            "Vkit icon resource validation failed: {error}. \
             Run `python build/windows/Generate-Icon.py --write` from the project root."
        );
    }

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("resources/windows.rc", embed_resource::NONE)
            .manifest_required()
            .expect("failed to compile Vkit Windows resources");
    }
}
