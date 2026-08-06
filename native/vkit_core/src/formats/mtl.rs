use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use super::{FormatError, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MtlOpacitySource {
    Dissolve,
    Transparency,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MtlMaterial {
    pub name: String,
    pub diffuse_color: Option<[f64; 3]>,

    pub opacity: Option<f64>,
    pub opacity_source: Option<MtlOpacitySource>,

    pub diffuse_map: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MtlDocument {
    pub materials: Vec<MtlMaterial>,
}

impl MtlDocument {
    pub fn diffuse_map_report(&self) -> DiffuseMapReport {
        let mut maps: Vec<DiffuseMapBinding> = Vec::new();
        for material in &self.materials {
            let Some(path) = &material.diffuse_map else {
                continue;
            };
            if let Some(existing) = maps.iter_mut().find(|entry| entry.path == *path) {
                existing.materials.push(material.name.clone());
            } else {
                maps.push(DiffuseMapBinding {
                    path: path.clone(),
                    materials: vec![material.name.clone()],
                });
            }
        }
        match maps.len() {
            0 => DiffuseMapReport::NoMap,
            1 => {
                let binding = maps.pop().expect("one map was counted");
                DiffuseMapReport::SingleMap {
                    path: binding.path,
                    materials: binding.materials,
                }
            }
            _ => DiffuseMapReport::MultipleMaps { maps },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffuseMapBinding {
    pub path: PathBuf,
    pub materials: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffuseMapReport {
    NoMap,
    SingleMap {
        path: PathBuf,
        materials: Vec<String>,
    },
    MultipleMaps {
        maps: Vec<DiffuseMapBinding>,
    },
}

pub fn load_mtl(path: impl AsRef<Path>) -> Result<MtlDocument> {
    let stream = BufReader::new(File::open(path)?);
    parse_mtl(stream)
}

pub fn parse_mtl(reader: impl BufRead) -> Result<MtlDocument> {
    let mut materials = Vec::new();
    let mut material_names = HashSet::new();
    let mut active: Option<MtlMaterial> = None;

    for (zero_based_line, line) in reader.lines().enumerate() {
        let line_number = zero_based_line + 1;
        let line = line?;
        let line = if line_number == 1 {
            line.trim_start_matches('\u{feff}')
        } else {
            line.as_str()
        };
        let content = line
            .split_once('#')
            .map_or(line, |(before, _)| before)
            .trim();
        if content.is_empty() {
            continue;
        }
        let keyword_end = content.find(char::is_whitespace).unwrap_or(content.len());
        let keyword = &content[..keyword_end];
        let value = content[keyword_end..].trim();

        match keyword {
            "newmtl" => {
                if value.is_empty() {
                    return Err(mtl_error(line_number, "newmtl requires a name"));
                }
                if value.chars().any(char::is_control) {
                    return Err(mtl_error(
                        line_number,
                        "material name contains a control character",
                    ));
                }
                if !material_names.insert(value.to_owned()) {
                    return Err(mtl_error(
                        line_number,
                        format!("duplicate material name {value:?}"),
                    ));
                }
                if let Some(material) = active.take() {
                    materials.push(material);
                }
                active = Some(MtlMaterial {
                    name: value.to_owned(),
                    diffuse_color: None,
                    opacity: None,
                    opacity_source: None,
                    diffuse_map: None,
                });
            }
            "Kd" => {
                let material = require_material(active.as_mut(), line_number, keyword)?;
                if material.diffuse_color.is_some() {
                    return Err(mtl_error(line_number, "duplicate Kd record"));
                }
                material.diffuse_color = Some(parse_unit_color(value, line_number)?);
            }
            "d" | "Tr" => {
                let material = require_material(active.as_mut(), line_number, keyword)?;
                if material.opacity.is_some() {
                    return Err(mtl_error(
                        line_number,
                        "material has more than one d/Tr opacity record",
                    ));
                }
                let raw = parse_unit_scalar(value, line_number, keyword)?;
                let (opacity, source) = if keyword == "d" {
                    (raw, MtlOpacitySource::Dissolve)
                } else {
                    (1.0 - raw, MtlOpacitySource::Transparency)
                };
                material.opacity = Some(opacity);
                material.opacity_source = Some(source);
            }
            "map_Kd" => {
                let material = require_material(active.as_mut(), line_number, keyword)?;
                if material.diffuse_map.is_some() {
                    return Err(mtl_error(line_number, "duplicate map_Kd record"));
                }
                if value.starts_with('-') {
                    return Err(mtl_error(
                        line_number,
                        "map_Kd options are outside the safe Phase A subset",
                    ));
                }
                let path = validate_local_asset_path(value)
                    .map_err(|message| mtl_error(line_number, message))?;
                material.diffuse_map = Some(path);
            }

            _ => {}
        }
    }

    if let Some(material) = active {
        materials.push(material);
    }
    Ok(MtlDocument { materials })
}

fn require_material<'a>(
    active: Option<&'a mut MtlMaterial>,
    line: usize,
    keyword: &str,
) -> Result<&'a mut MtlMaterial> {
    active.ok_or_else(|| mtl_error(line, format!("{keyword} appears before newmtl")))
}

fn parse_unit_color(value: &str, line: usize) -> Result<[f64; 3]> {
    let fields = value.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 3 {
        return Err(mtl_error(line, "Kd requires exactly three values"));
    }
    Ok([
        parse_unit_number(fields[0], line, "Kd")?,
        parse_unit_number(fields[1], line, "Kd")?,
        parse_unit_number(fields[2], line, "Kd")?,
    ])
}

fn parse_unit_scalar(value: &str, line: usize, label: &str) -> Result<f64> {
    let fields = value.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 1 {
        return Err(mtl_error(
            line,
            format!("{label} requires exactly one value"),
        ));
    }
    parse_unit_number(fields[0], line, label)
}

fn parse_unit_number(raw: &str, line: usize, label: &str) -> Result<f64> {
    let value = raw
        .parse::<f64>()
        .map_err(|_| mtl_error(line, format!("invalid {label} value {raw:?}")))?;
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(mtl_error(
            line,
            format!("{label} value must be finite and within [0, 1]"),
        ));
    }
    Ok(value)
}

pub(crate) fn validate_local_asset_path(raw: &str) -> std::result::Result<PathBuf, String> {
    if raw.is_empty() {
        return Err("asset path is empty".to_owned());
    }
    if raw.trim() != raw {
        return Err("asset path has leading or trailing whitespace".to_owned());
    }
    if raw.chars().any(char::is_control) {
        return Err("asset path contains a control character".to_owned());
    }
    if raw.contains(':') {
        return Err("asset path must not contain a drive, URL scheme, or data stream".to_owned());
    }

    let normalized = raw.replace('\\', "/");
    if normalized.starts_with('/') {
        return Err("asset path must be relative and must not be UNC/rooted".to_owned());
    }

    let mut safe = PathBuf::new();
    for component in normalized.split('/') {
        if component.is_empty() {
            return Err("asset path contains an empty component".to_owned());
        }
        if component == ".." {
            return Err("asset path must not traverse a parent directory".to_owned());
        }
        if component == "." {
            continue;
        }
        if component.ends_with('.') || component.ends_with(' ') {
            return Err("asset path component has an unsafe Windows suffix".to_owned());
        }
        if is_windows_device_name(component) {
            return Err("asset path names a reserved Windows device".to_owned());
        }
        safe.push(component);
    }
    if safe.as_os_str().is_empty() {
        return Err("asset path does not name a file".to_owned());
    }
    Ok(safe)
}

fn is_windows_device_name(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or(component);
    let upper = stem.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && matches!(upper.as_bytes()[3], b'1'..=b'9'))
}

fn mtl_error(line: usize, message: impl Into<String>) -> FormatError {
    FormatError::InvalidMtl {
        line,
        message: message.into(),
    }
}
