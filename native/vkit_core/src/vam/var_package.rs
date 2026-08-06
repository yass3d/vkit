use std::path::{Path, PathBuf};

use super::zip_archive::ZipWriter;
use super::{Result, VaMError};

#[derive(Clone, Debug)]
pub struct VarMetadata {
    pub creator: String,
    pub package: String,

    pub version: u32,
    pub license: String,
    pub description: String,
    pub credits: String,
    pub instructions: String,

    pub promotional_link: String,

    pub program_version: String,
}

impl Default for VarMetadata {
    fn default() -> Self {
        Self {
            creator: String::new(),
            package: String::new(),
            version: 1,
            license: "CC BY".to_owned(),
            description: String::new(),
            credits: String::new(),
            instructions: String::new(),
            promotional_link: String::new(),
            program_version: format!("Vkit {}", env!("CARGO_PKG_VERSION")),
        }
    }
}

pub const VAR_LICENSES: [&str; 7] = [
    "CC BY",
    "CC BY-SA",
    "CC BY-ND",
    "CC BY-NC",
    "CC BY-NC-SA",
    "CC BY-NC-ND",
    "PC EA",
];

#[derive(Clone, Debug)]
pub struct VarContent {
    pub internal_path: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct VarPackage {
    pub path: PathBuf,
    pub version: u32,
    pub contents: Vec<String>,
}

#[must_use]
pub fn safe_identity(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "Unnamed".to_owned()
    } else {
        trimmed.to_owned()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ExistingPackage {
    #[default]
    Keep,

    Replace,
}

#[must_use]
pub fn var_package_path(directory: &Path, metadata: &VarMetadata) -> PathBuf {
    directory.join(format!(
        "{}.{}.{}.var",
        safe_identity(&metadata.creator),
        safe_identity(&metadata.package),
        metadata.version
    ))
}

#[must_use]
pub fn next_version(directory: &Path, creator: &str, package: &str) -> u32 {
    let prefix = format!("{creator}.{package}.");
    let highest = std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|name| {
            let rest = name.strip_prefix(&prefix)?;
            rest.strip_suffix(".var")?.parse::<u32>().ok()
        })
        .max();
    highest.map_or(1, |version| version.saturating_add(1))
}

pub fn write_var_package(
    directory: &Path,
    metadata: &VarMetadata,
    contents: &[VarContent],
    existing: ExistingPackage,
) -> Result<VarPackage> {
    if contents.is_empty() {
        return Err(package_error("a package needs at least one file"));
    }
    let mut seen = std::collections::BTreeSet::new();
    for content in contents {
        validate_internal_path(&content.internal_path)?;

        if !seen.insert(content.internal_path.as_str()) {
            return Err(package_error(format!(
                "{} appears twice in this package",
                content.internal_path
            )));
        }
    }
    if metadata.creator.trim().is_empty() {
        return Err(package_error("a package needs a creator name"));
    }
    if metadata.package.trim().is_empty() {
        return Err(package_error("a package needs a package name"));
    }
    let creator = safe_identity(&metadata.creator);
    let package = safe_identity(&metadata.package);
    std::fs::create_dir_all(directory)
        .map_err(|error| super::io_error(directory.to_path_buf(), error))?;
    let version = metadata.version;
    let path = var_package_path(directory, metadata);

    if existing == ExistingPackage::Keep && path.exists() {
        return Err(package_error(format!("{} already exists", path.display())));
    }

    let listing: Vec<String> = contents
        .iter()
        .map(|content| content.internal_path.clone())
        .collect();
    let manifest = meta_json(&creator, &package, metadata, &listing);

    let mut archive = ZipWriter::default();
    archive.add("meta.json", manifest.as_bytes());
    for content in contents {
        archive.add(&content.internal_path, &content.bytes);
    }
    let bytes = archive.finish();

    let scratch = path.with_extension("var.partial");
    std::fs::write(&scratch, &bytes).map_err(|error| super::io_error(scratch.clone(), error))?;
    std::fs::rename(&scratch, &path).map_err(|error| {
        let _ = std::fs::remove_file(&scratch);
        super::io_error(path.clone(), error)
    })?;

    Ok(VarPackage {
        path,
        version,
        contents: listing,
    })
}

fn validate_internal_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.split('/').any(|part| part.is_empty() || part == "..")
    {
        return Err(package_error(format!(
            "{path} is not a usable path inside a package"
        )));
    }
    Ok(())
}

fn meta_json(creator: &str, package: &str, metadata: &VarMetadata, contents: &[String]) -> String {
    let mut json = String::new();
    json.push_str("{\n");
    json.push_str(&format!(
        "  \"licenseType\" : {},\n",
        quote(&metadata.license)
    ));
    json.push_str(&format!("  \"creatorName\" : {},\n", quote(creator)));
    json.push_str(&format!("  \"packageName\" : {},\n", quote(package)));
    json.push_str("  \"standardReferenceVersionOption\" : \"Latest\",\n");
    json.push_str("  \"scriptReferenceVersionOption\" : \"Exact\",\n");
    json.push_str(&format!(
        "  \"description\" : {},\n",
        quote(&metadata.description)
    ));
    json.push_str(&format!("  \"credits\" : {},\n", quote(&metadata.credits)));
    json.push_str(&format!(
        "  \"instructions\" : {},\n",
        quote(&metadata.instructions)
    ));
    json.push_str(&format!(
        "  \"promotionalLink\" : {},\n",
        quote(&metadata.promotional_link)
    ));
    json.push_str(&format!(
        "  \"programVersion\" : {},\n",
        quote(&metadata.program_version)
    ));
    json.push_str("  \"contentList\" : [\n");
    for (index, entry) in contents.iter().enumerate() {
        let comma = if index + 1 == contents.len() { "" } else { "," };
        json.push_str(&format!("    {}{comma}\n", quote(entry)));
    }
    json.push_str("  ],\n");

    json.push_str("  \"dependencies\" : {},\n");

    let preloads_morphs = contents.iter().any(|entry| entry.ends_with(".vmi"));
    json.push_str(&format!(
        "  \"customOptions\" : {{\n    \"preloadMorphs\" : \"{preloads_morphs}\"\n  }},\n"
    ));
    json.push_str("  \"hadReferenceIssues\" : \"false\",\n");
    json.push_str("  \"referenceIssues\" : []\n");
    json.push_str("}\n");
    json
}

fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if control < ' ' => out.push_str(&format!("\\u{:04x}", control as u32)),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn package_error(message: impl Into<String>) -> VaMError {
    VaMError::InvalidSkinPreset {
        locator: "var package".to_owned(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_that_would_break_a_filename_is_made_safe() {
        assert_eq!(safe_identity("My Creator"), "My_Creator");
        assert_eq!(safe_identity("a.b/c\\d"), "a_b_c_d");
        assert_eq!(safe_identity("   "), "Unnamed");
        assert_eq!(safe_identity("__Edge__"), "Edge");
        assert_eq!(safe_identity("Vkit"), "Vkit");
    }

    #[test]
    fn a_path_that_escapes_the_archive_is_refused() {
        for bad in [
            "",
            "/Custom/x.png",
            "Custom\\x.png",
            "Custom/../x.png",
            "Custom//x.png",
        ] {
            assert!(
                validate_internal_path(bad).is_err(),
                "{bad} should be refused"
            );
        }
        assert!(validate_internal_path("Custom/Atom/Person/Morphs/female/a.vmi").is_ok());
    }

    #[test]
    fn a_package_carrying_a_morph_asks_for_it_to_be_preloaded() {
        let metadata = VarMetadata::default();
        let preload = |contents: &[&str]| {
            let owned: Vec<String> = contents.iter().map(|entry| (*entry).to_owned()).collect();
            let json = meta_json("Someone", "Face", &metadata, &owned);
            let parsed: serde_json::Value =
                serde_json::from_str(&json).expect("the manifest is JSON");
            parsed["customOptions"]["preloadMorphs"]
                .as_str()
                .expect("preloadMorphs is a quoted boolean, as VaM writes it")
                .to_owned()
        };

        assert_eq!(
            preload(&[
                "Custom/Atom/Person/Morphs/female/a.vmi",
                "Custom/Atom/Person/Morphs/female/a.vmb",
            ]),
            "true",
            "a morph nobody preloads is a morph nobody can select"
        );

        assert_eq!(preload(&["Custom/Atom/Person/Textures/face.png"]), "false");
    }

    #[test]
    fn the_manifest_carries_what_vam_reads() {
        let metadata = VarMetadata {
            creator: "Someone".to_owned(),
            package: "Face".to_owned(),
            description: "a \"quoted\" line\nand another".to_owned(),
            promotional_link: "https://example.invalid/support".to_owned(),
            ..VarMetadata::default()
        };
        let json = meta_json("Someone", "Face", &metadata, &["Custom/a.vmi".to_owned()]);

        for field in [
            "licenseType",
            "creatorName",
            "packageName",
            "standardReferenceVersionOption",
            "scriptReferenceVersionOption",
            "description",
            "credits",
            "instructions",
            "promotionalLink",
            "programVersion",
            "contentList",
            "dependencies",
            "customOptions",
            "preloadMorphs",
            "hadReferenceIssues",
            "referenceIssues",
        ] {
            assert!(json.contains(&format!("\"{field}\"")), "{field} is missing");
        }

        assert!(json.contains(r#"a \"quoted\" line\nand another"#));
        assert!(
            serde_json::from_str::<serde_json::Value>(&json).is_ok(),
            "{json}"
        );
    }

    #[test]
    fn versions_step_past_whatever_is_already_there() {
        let directory = tempfile::tempdir().expect("a temp dir");
        assert_eq!(next_version(directory.path(), "Me", "Face"), 1);
        for name in ["Me.Face.1.var", "Me.Face.4.var", "Me.Other.9.var", "junk"] {
            std::fs::write(directory.path().join(name), b"x").expect("written");
        }
        assert_eq!(next_version(directory.path(), "Me", "Face"), 5);
        assert_eq!(next_version(directory.path(), "Me", "Other"), 10);
    }

    #[test]
    fn a_written_package_reads_back_through_our_own_var_reader() {
        let directory = tempfile::tempdir().expect("a temp dir");

        let morph = b"{\"morph\":\"compress me, this text repeats and repeats\"}".to_vec();
        let texture = vec![7_u8; 4096];
        let written = write_var_package(
            directory.path(),
            &VarMetadata {
                creator: "Some One".to_owned(),
                package: "My Face".to_owned(),
                ..VarMetadata::default()
            },
            &[
                VarContent {
                    internal_path: "Custom/Atom/Person/Morphs/female/MyFace.vmi".to_owned(),
                    bytes: morph.clone(),
                },
                VarContent {
                    internal_path: "Custom/Atom/Person/Textures/MyFace/face.png".to_owned(),
                    bytes: texture.clone(),
                },
            ],
            ExistingPackage::Keep,
        )
        .expect("the package writes");

        assert_eq!(
            written.path.file_name().and_then(|name| name.to_str()),
            Some("Some_One.My_Face.1.var")
        );

        let entries = crate::vam::list_var_entries(&written.path).expect("it lists");
        assert!(entries.iter().any(|entry| entry == "meta.json"));
        assert!(
            entries
                .iter()
                .any(|entry| entry.ends_with("Morphs/female/MyFace.vmi"))
        );

        let read_back = crate::vam::read_var_entry_bytes(
            &written.path,
            "Custom/Atom/Person/Morphs/female/MyFace.vmi",
            1 << 20,
        )
        .expect("the deflated entry reads");
        assert_eq!(read_back, morph, "deflate round-trips");
        let read_texture = crate::vam::read_var_entry_bytes(
            &written.path,
            "Custom/Atom/Person/Textures/MyFace/face.png",
            1 << 20,
        )
        .expect("the stored entry reads");
        assert_eq!(read_texture, texture, "stored round-trips");

        let manifest = crate::vam::read_var_entry_bytes(&written.path, "meta.json", 1 << 20)
            .expect("the manifest reads");
        let parsed: serde_json::Value =
            serde_json::from_slice(&manifest).expect("the manifest is JSON");
        assert_eq!(parsed["creatorName"], "Some_One");
        assert_eq!(parsed["contentList"].as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn a_package_without_a_creator_or_a_name_is_refused() {
        let directory = tempfile::tempdir().expect("a temp dir");
        let content = vec![VarContent {
            internal_path: "Custom/x.vmi".to_owned(),
            bytes: b"x".to_vec(),
        }];
        let write = |metadata: &VarMetadata| {
            write_var_package(directory.path(), metadata, &content, ExistingPackage::Keep)
        };
        assert!(write(&VarMetadata::default()).is_err(), "both empty");
        assert!(
            write(&VarMetadata {
                creator: "Me".to_owned(),
                ..VarMetadata::default()
            })
            .is_err(),
            "no package name"
        );
        assert!(
            write(&VarMetadata {
                package: "Face".to_owned(),
                ..VarMetadata::default()
            })
            .is_err(),
            "no creator"
        );
    }

    #[test]
    fn each_version_writes_its_own_file() {
        let directory = tempfile::tempdir().expect("a temp dir");
        let content = || {
            vec![VarContent {
                internal_path: "Custom/x.vmi".to_owned(),
                bytes: b"x".to_vec(),
            }]
        };
        let named = |version| VarMetadata {
            version,
            ..named_metadata()
        };
        let first = write_var_package(
            directory.path(),
            &named(1),
            &content(),
            ExistingPackage::Keep,
        )
        .expect("first");
        let second = write_var_package(
            directory.path(),
            &named(2),
            &content(),
            ExistingPackage::Keep,
        )
        .expect("second");
        assert_eq!((first.version, second.version), (1, 2));
        assert!(first.path.exists() && second.path.exists());
        assert_eq!(next_version(directory.path(), "Me", "Face"), 3);
    }

    #[test]
    fn the_same_path_twice_is_refused() {
        let directory = tempfile::tempdir().expect("a temp dir");
        let twice = vec![
            VarContent {
                internal_path: "Custom/a.vmi".to_owned(),
                bytes: b"first".to_vec(),
            },
            VarContent {
                internal_path: "Custom/a.vmi".to_owned(),
                bytes: b"second".to_vec(),
            },
        ];
        assert!(
            write_var_package(
                directory.path(),
                &named_metadata(),
                &twice,
                ExistingPackage::Keep
            )
            .is_err()
        );
    }

    #[test]
    fn an_existing_package_file_is_replaced_only_on_request() {
        let directory = tempfile::tempdir().expect("a temp dir");
        let content = vec![VarContent {
            internal_path: "Custom/a.vmi".to_owned(),
            bytes: b"x".to_vec(),
        }];
        let occupied = var_package_path(directory.path(), &named_metadata());
        std::fs::write(&occupied, b"not a package").expect("occupied");

        assert!(
            write_var_package(
                directory.path(),
                &named_metadata(),
                &content,
                ExistingPackage::Keep
            )
            .is_err(),
            "the default refuses"
        );
        assert_eq!(
            std::fs::read(&occupied).expect("untouched"),
            b"not a package",
            "the existing file is still what it was"
        );

        let written = write_var_package(
            directory.path(),
            &named_metadata(),
            &content,
            ExistingPackage::Replace,
        )
        .expect("a confirmed replace goes through");
        assert_eq!(written.path, occupied);
        assert_ne!(
            std::fs::read(&occupied).expect("replaced"),
            b"not a package"
        );
    }

    #[test]
    fn a_package_with_nothing_in_it_is_refused() {
        let directory = tempfile::tempdir().expect("a temp dir");
        assert!(
            write_var_package(
                directory.path(),
                &named_metadata(),
                &[],
                ExistingPackage::Keep
            )
            .is_err()
        );
    }

    #[test]
    fn the_manifest_carries_the_licence_code_alone() {
        let directory = tempfile::tempdir().expect("a temp dir");
        for license in VAR_LICENSES {
            let written = write_var_package(
                directory.path(),
                &VarMetadata {
                    license: license.to_owned(),
                    version: 1,
                    package: license.replace([' ', '-'], "_"),
                    ..named_metadata()
                },
                &[VarContent {
                    internal_path: "Custom/x.vmi".to_owned(),
                    bytes: b"x".to_vec(),
                }],
                ExistingPackage::Keep,
            )
            .expect("it writes");
            let manifest = crate::vam::read_var_entry_bytes(&written.path, "meta.json", 1 << 20)
                .expect("the manifest reads");
            let parsed: serde_json::Value =
                serde_json::from_slice(&manifest).expect("the manifest is JSON");
            assert_eq!(parsed["licenseType"], license, "{license}");
        }
    }

    fn named_metadata() -> VarMetadata {
        VarMetadata {
            creator: "Me".to_owned(),
            package: "Face".to_owned(),
            ..VarMetadata::default()
        }
    }
}
