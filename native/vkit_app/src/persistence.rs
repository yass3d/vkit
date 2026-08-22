use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use vkit_core::{
    formats::FormatError,
    surface_smoothing::{VAM_SMOOTH_PASSES_DEFAULT, VAM_SMOOTH_PASSES_MAX},
    vam::{decode_vmb_daz_cm, parse_vmi, vmb_raw_entry_count},
};

use crate::{
    i18n::Locale,
    lighting::{DEFAULT_LIGHT_BRIGHTNESS, LightingPreset, sanitize_brightness},
    post_process::VignetteSettings,
    shader_color::ToneMapping,
    state::{
        BaseViewMode, DEFAULT_CUSTOM_HEAD_SOLID_COLOR_RGB, DEFAULT_G2_SOLID_COLOR_RGB,
        DEFAULT_WIREFRAME_COLOR_RGB, FigureSex, MorphNameDisplay, ViewportBackgroundMode,
    },
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub const MAX_VAM_PAIR_FILE_BYTES: u64 = 20 * 1024 * 1024;

pub fn vam_morph_pair_sibling_paths(selected: &Path) -> Option<(PathBuf, PathBuf)> {
    let extension = selected.extension().and_then(|value| value.to_str())?;
    let sibling = |target: &str| {
        let mut path = selected.to_path_buf();
        path.set_extension(target);
        path
    };
    if extension.eq_ignore_ascii_case("vmi") {
        Some((selected.to_path_buf(), sibling("vmb")))
    } else if extension.eq_ignore_ascii_case("vmb") {
        Some((sibling("vmi"), selected.to_path_buf()))
    } else {
        None
    }
}

pub fn read_bounded_vam_pair_file(path: &Path, label: &str) -> Result<Vec<u8>, String> {
    let size = fs::metadata(path)
        .map_err(|error| format!("Could not inspect {label} {}: {error}", path.display()))?
        .len();
    if size > MAX_VAM_PAIR_FILE_BYTES {
        return Err(format!(
            "{label} is {size} bytes; the import safety limit is {MAX_VAM_PAIR_FILE_BYTES} bytes"
        ));
    }
    fs::read(path).map_err(|error| format!("Could not read {label} {}: {error}", path.display()))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(
        default,
        deserialize_with = "permissive_optional_enum",
        skip_serializing_if = "Option::is_none"
    )]
    pub locale: Option<Locale>,

    #[serde(default)]
    pub morph_name_display: MorphNameDisplay,

    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub shortcuts: std::collections::BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inspector_width: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vam_root: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vam_geometry_base_path: Option<PathBuf>,
    #[serde(
        default,
        deserialize_with = "permissive_optional_enum",
        skip_serializing_if = "Option::is_none"
    )]
    pub figure_sex: Option<FigureSex>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vam_export_display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vam_export_group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vam_export_region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vam_export_is_pose_control: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vam_export_bone_correction: Option<bool>,
    #[serde(
        default = "default_custom_head_solid_color_rgb",
        deserialize_with = "deserialize_custom_head_solid_color_rgb"
    )]
    pub custom_head_solid_color_rgb: [u8; 3],
    #[serde(
        default = "default_g2_solid_color_rgb",
        deserialize_with = "deserialize_g2_solid_color_rgb"
    )]
    pub g2_solid_color_rgb: [u8; 3],
    #[serde(
        default = "default_wireframe_color_rgb",
        deserialize_with = "deserialize_wireframe_color_rgb"
    )]
    pub wireframe_color_rgb: [u8; 3],
    #[serde(default, deserialize_with = "permissive_enum")]
    pub base_view_mode: BaseViewMode,
    #[serde(default = "default_surface_smooth_passes")]
    pub surface_smooth_passes: u8,
    #[serde(default = "default_tooltips_enabled")]
    pub tooltips_enabled: bool,
    #[serde(default = "default_hair_toolbox_columns")]
    pub hair_toolbox_columns: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hair_toolbox_pos: Option<[f32; 2]>,
    #[serde(default = "default_show_one_sided_morphs")]
    pub show_one_sided_morphs: bool,

    #[serde(default)]
    pub last_skin_id: Option<String>,

    #[serde(default)]
    pub default_skin_id: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_creator: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_promotional_link: Option<String>,
    #[serde(default, deserialize_with = "permissive_enum")]
    pub viewport_background_mode: ViewportBackgroundMode,
    #[serde(default)]
    pub wireframe_visible: bool,
    #[serde(default = "default_wireframe_opacity")]
    pub wireframe_opacity: f32,
    #[serde(default)]
    pub xray_visible: bool,
    #[serde(default = "default_xray_opacity")]
    pub xray_opacity: f32,
    #[serde(default)]
    pub scan_overlay: bool,
    #[serde(default = "default_overlay_opacity")]
    pub overlay_opacity: f32,
    #[serde(default = "default_result_tear_lacrimals")]
    pub show_result_tear_lacrimals: bool,
    #[serde(default = "default_result_eyelashes")]
    pub show_result_eyelashes: bool,
    #[serde(default = "default_alignment_opacity")]
    pub alignment_opacity: f32,
    #[serde(default = "default_alignment_g2_opacity")]
    pub alignment_g2_opacity: f32,
    #[serde(default)]
    pub light_yaw_radians: f32,
    #[serde(default, deserialize_with = "permissive_enum")]
    pub lighting_preset: LightingPreset,
    #[serde(default = "default_light_brightness")]
    pub light_brightness: f32,

    #[serde(default)]
    pub tone_mapping: u8,

    #[serde(default)]
    pub vignette_enabled: bool,
    #[serde(default = "default_vignette_intensity")]
    pub vignette_intensity: f32,
    #[serde(default = "default_vignette_smoothness")]
    pub vignette_smoothness: f32,
    #[serde(default = "default_vignette_roundness")]
    pub vignette_roundness: f32,
}

const fn default_vignette_intensity() -> f32 {
    crate::post_process::DEFAULT_VIGNETTE_INTENSITY
}

const fn default_vignette_smoothness() -> f32 {
    crate::post_process::DEFAULT_VIGNETTE_SMOOTHNESS
}

const fn default_vignette_roundness() -> f32 {
    crate::post_process::DEFAULT_VIGNETTE_ROUNDNESS
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            locale: None,
            hair_toolbox_columns: default_hair_toolbox_columns(),
            hair_toolbox_pos: None,
            morph_name_display: MorphNameDisplay::default(),
            shortcuts: std::collections::BTreeMap::new(),
            package_creator: None,
            package_version: None,
            package_license: None,
            package_promotional_link: None,
            inspector_width: None,
            vam_root: None,
            vam_geometry_base_path: None,
            figure_sex: None,
            vam_export_display_name: None,
            vam_export_group: None,
            vam_export_region: None,
            vam_export_is_pose_control: None,
            vam_export_bone_correction: None,
            custom_head_solid_color_rgb: DEFAULT_CUSTOM_HEAD_SOLID_COLOR_RGB,
            g2_solid_color_rgb: DEFAULT_G2_SOLID_COLOR_RGB,
            wireframe_color_rgb: DEFAULT_WIREFRAME_COLOR_RGB,
            base_view_mode: BaseViewMode::default(),
            surface_smooth_passes: default_surface_smooth_passes(),
            tooltips_enabled: default_tooltips_enabled(),
            show_one_sided_morphs: default_show_one_sided_morphs(),
            last_skin_id: None,
            default_skin_id: None,
            viewport_background_mode: ViewportBackgroundMode::default(),
            wireframe_visible: false,
            wireframe_opacity: default_wireframe_opacity(),
            xray_visible: false,
            xray_opacity: default_xray_opacity(),
            scan_overlay: false,
            overlay_opacity: default_overlay_opacity(),
            show_result_tear_lacrimals: default_result_tear_lacrimals(),
            show_result_eyelashes: default_result_eyelashes(),
            alignment_opacity: default_alignment_opacity(),
            alignment_g2_opacity: default_alignment_g2_opacity(),
            light_yaw_radians: 0.0,
            lighting_preset: LightingPreset::default(),
            light_brightness: default_light_brightness(),
            tone_mapping: ToneMapping::default().id(),
            vignette_enabled: VignetteSettings::default().enabled,
            vignette_intensity: default_vignette_intensity(),
            vignette_smoothness: default_vignette_smoothness(),
            vignette_roundness: default_vignette_roundness(),
        }
    }
}

impl Preferences {
    pub fn sanitize(&mut self) {
        self.custom_head_solid_color_rgb =
            sanitize_solid_color_rgb(self.custom_head_solid_color_rgb);
        self.g2_solid_color_rgb = sanitize_solid_color_rgb(self.g2_solid_color_rgb);
        self.wireframe_color_rgb = sanitize_solid_color_rgb(self.wireframe_color_rgb);
        self.surface_smooth_passes = self.surface_smooth_passes.min(VAM_SMOOTH_PASSES_MAX);
        self.wireframe_opacity = sanitize_unit(self.wireframe_opacity, default_wireframe_opacity());
        self.xray_opacity = sanitize_unit(self.xray_opacity, default_xray_opacity());
        self.overlay_opacity = sanitize_unit(self.overlay_opacity, default_overlay_opacity());
        self.alignment_opacity = sanitize_unit(self.alignment_opacity, default_alignment_opacity());
        self.alignment_g2_opacity =
            sanitize_unit(self.alignment_g2_opacity, default_alignment_g2_opacity());
        self.light_yaw_radians = if self.light_yaw_radians.is_finite() {
            self.light_yaw_radians.rem_euclid(std::f32::consts::TAU)
        } else {
            0.0
        };
        self.light_brightness = sanitize_brightness(self.light_brightness);
    }

    pub fn with_sanitized_display(mut self) -> Self {
        self.sanitize();
        self
    }
}

fn default_custom_head_solid_color_rgb() -> [u8; 3] {
    DEFAULT_CUSTOM_HEAD_SOLID_COLOR_RGB
}

fn default_g2_solid_color_rgb() -> [u8; 3] {
    DEFAULT_G2_SOLID_COLOR_RGB
}

fn default_wireframe_color_rgb() -> [u8; 3] {
    DEFAULT_WIREFRAME_COLOR_RGB
}

const fn default_surface_smooth_passes() -> u8 {
    VAM_SMOOTH_PASSES_DEFAULT
}

const fn default_tooltips_enabled() -> bool {
    true
}

const fn default_hair_toolbox_columns() -> u8 {
    1
}

const fn default_show_one_sided_morphs() -> bool {
    false
}

const fn default_wireframe_opacity() -> f32 {
    0.32
}

const fn default_xray_opacity() -> f32 {
    0.28
}

const fn default_overlay_opacity() -> f32 {
    0.5
}

const fn default_result_tear_lacrimals() -> bool {
    true
}

const fn default_result_eyelashes() -> bool {
    true
}

const fn default_alignment_opacity() -> f32 {
    1.0
}

const fn default_alignment_g2_opacity() -> f32 {
    1.0
}

const fn default_light_brightness() -> f32 {
    DEFAULT_LIGHT_BRIGHTNESS
}

fn permissive_enum<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned + Default,
{
    let value = String::deserialize(deserializer)?;
    Ok(
        serde_json::from_value(serde_json::Value::String(value.trim().to_ascii_lowercase()))
            .unwrap_or_default(),
    )
}

fn permissive_optional_enum<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned + Default,
{
    Ok(Option::<String>::deserialize(deserializer)?.map(|value| {
        serde_json::from_value(serde_json::Value::String(value.trim().to_ascii_lowercase()))
            .unwrap_or_default()
    }))
}

fn sanitize_unit(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        fallback
    }
}

fn sanitize_solid_color_rgb(color: [u8; 3]) -> [u8; 3] {
    color
}

fn deserialize_custom_head_solid_color_rgb<'de, D>(deserializer: D) -> Result<[u8; 3], D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(deserialize_solid_color_value(
        value,
        DEFAULT_CUSTOM_HEAD_SOLID_COLOR_RGB,
    ))
}

fn deserialize_g2_solid_color_rgb<'de, D>(deserializer: D) -> Result<[u8; 3], D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(deserialize_solid_color_value(
        value,
        DEFAULT_G2_SOLID_COLOR_RGB,
    ))
}

fn deserialize_wireframe_color_rgb<'de, D>(deserializer: D) -> Result<[u8; 3], D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(deserialize_solid_color_value(
        value,
        DEFAULT_WIREFRAME_COLOR_RGB,
    ))
}

fn deserialize_solid_color_value(value: serde_json::Value, fallback: [u8; 3]) -> [u8; 3] {
    let Some(values) = value.as_array() else {
        return fallback;
    };
    if values.len() != 3 {
        return fallback;
    }

    let mut color = [0; 3];
    for (component, value) in color.iter_mut().zip(values) {
        let Some(number) = value.as_f64() else {
            return fallback;
        };
        if !number.is_finite() {
            return fallback;
        }
        *component = number.round().clamp(0.0, 255.0) as u8;
    }
    color
}

#[derive(Clone, Debug)]
pub struct PreferenceStore {
    path: PathBuf,
}

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("file I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("settings data is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("OBJ data is invalid: {0}")]
    Obj(#[from] FormatError),
    #[error("the destination has no parent directory: {0}")]
    MissingParent(PathBuf),
    #[error("the VMI and VMB destinations must be distinct siblings in one directory")]
    InvalidMorphPairDestination,
    #[error("VaM morph pair data is invalid: {0}")]
    InvalidMorphPair(String),
}

impl PreferenceStore {
    pub fn discover() -> Option<Self> {
        let root = env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
        let directory = root.join(crate::APP_NAME);
        Some(Self {
            path: directory.join("settings.json"),
        })
    }

    #[cfg(test)]
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Result<Preferences, PersistenceError> {
        match fs::read(&self.path) {
            Ok(bytes) => {
                Ok(serde_json::from_slice::<Preferences>(&bytes)?.with_sanitized_display())
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Preferences::default()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn save(&self, preferences: &Preferences) -> Result<(), PersistenceError> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| PersistenceError::MissingParent(self.path.clone()))?;
        fs::create_dir_all(parent)?;
        let bytes = serde_json::to_vec(&preferences.clone().with_sanitized_display())?;
        atomic_write(&self.path, |file| {
            file.write_all(&bytes)?;
            Ok(())
        })
    }
}

pub fn write_vam_morph_pair_atomic(
    vmi_path: impl AsRef<Path>,
    vmi_bytes: &[u8],
    vmb_path: impl AsRef<Path>,
    vmb_bytes: &[u8],
) -> Result<(), PersistenceError> {
    let vmi_path = vmi_path.as_ref();
    let vmb_path = vmb_path.as_ref();
    let parent = vmi_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| PersistenceError::MissingParent(vmi_path.to_path_buf()))?;
    let vmb_parent = vmb_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| PersistenceError::MissingParent(vmb_path.to_path_buf()))?;
    let matching_stem =
        vmi_path.file_stem().is_some() && vmi_path.file_stem() == vmb_path.file_stem();
    let vmi_extension = vmi_path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("vmi"));
    let vmb_extension = vmb_path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("vmb"));
    if parent != vmb_parent
        || vmi_path == vmb_path
        || !matching_stem
        || !vmi_extension
        || !vmb_extension
        || !parent.is_dir()
    {
        return Err(PersistenceError::InvalidMorphPairDestination);
    }
    validate_vam_pair_bytes(vmi_bytes, vmb_bytes)?;

    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let staged_vmi = private_pair_path(vmi_path, sequence, "vmi-stage");
    let staged_vmb = private_pair_path(vmb_path, sequence, "vmb-stage");
    let backup_vmi = private_pair_path(vmi_path, sequence, "vmi-backup");
    let backup_vmb = private_pair_path(vmb_path, sequence, "vmb-backup");
    let mut staged_cleanup = PairStageCleanup(vec![staged_vmi.clone(), staged_vmb.clone()]);

    write_synced_new(&staged_vmi, vmi_bytes)?;
    write_synced_new(&staged_vmb, vmb_bytes)?;
    let staged_vmi_bytes = fs::read(&staged_vmi)?;
    let staged_vmb_bytes = fs::read(&staged_vmb)?;
    validate_vam_pair_bytes(&staged_vmi_bytes, &staged_vmb_bytes)?;
    if staged_vmi_bytes != vmi_bytes || staged_vmb_bytes != vmb_bytes {
        return Err(PersistenceError::InvalidMorphPair(
            "staged sibling bytes changed before commit".to_owned(),
        ));
    }

    let had_vmi = vmi_path.is_file();
    let had_vmb = vmb_path.is_file();
    if had_vmi {
        fs::rename(vmi_path, &backup_vmi)?;
    }
    if had_vmb && let Err(error) = fs::rename(vmb_path, &backup_vmb) {
        if had_vmi && let Err(restore_error) = fs::rename(&backup_vmi, vmi_path) {
            let _ = crate::diagnostics::record(
                crate::diagnostics::Severity::Error,
                "persistence",
                "vam_pair_backup_restore_failed",
                &format!(
                    "{} could not be restored from {}: {restore_error}",
                    vmi_path.display(),
                    backup_vmi.display()
                ),
            );
            return Err(PersistenceError::InvalidMorphPair(format!(
                "backup failed ({error}); restoring the prior VMI also failed ({restore_error}); recover it from {}",
                backup_vmi.display()
            )));
        }
        return Err(error.into());
    }

    let mut installed_vmi = false;
    let mut installed_vmb = false;
    let promotion = (|| -> io::Result<()> {
        fs::rename(&staged_vmi, vmi_path)?;
        installed_vmi = true;
        fs::rename(&staged_vmb, vmb_path)?;
        installed_vmb = true;
        Ok(())
    })();
    if let Err(commit_error) = promotion {
        let rollback = rollback_morph_pair(
            MorphPairFile {
                path: vmi_path,
                backup: &backup_vmi,
                existed: had_vmi,
                installed: installed_vmi,
            },
            MorphPairFile {
                path: vmb_path,
                backup: &backup_vmb,
                existed: had_vmb,
                installed: installed_vmb,
            },
        );
        return Err(match rollback {
            Ok(()) => PersistenceError::Io(commit_error),
            Err(rollback_error) => PersistenceError::InvalidMorphPair(format!(
                "commit failed ({commit_error}); rollback also failed ({rollback_error}); recover prior files from {} and {}",
                backup_vmi.display(),
                backup_vmb.display()
            )),
        });
    }

    staged_cleanup.0.clear();
    if had_vmi {
        let _ = fs::remove_file(&backup_vmi);
    }
    if had_vmb {
        let _ = fs::remove_file(&backup_vmb);
    }
    Ok(())
}

fn validate_vam_pair_bytes(vmi_bytes: &[u8], vmb_bytes: &[u8]) -> Result<(), PersistenceError> {
    let metadata = parse_vmi(vmi_bytes)
        .map_err(|error| PersistenceError::InvalidMorphPair(error.to_string()))?;
    let raw_entry_count = vmb_raw_entry_count(vmb_bytes)
        .map_err(|error| PersistenceError::InvalidMorphPair(error.to_string()))?;
    let declared_count = metadata.num_deltas_value().ok_or_else(|| {
        PersistenceError::InvalidMorphPair(
            "VMI numDeltas is required for a committed VMI/VMB pair".to_owned(),
        )
    })?;
    if declared_count != raw_entry_count as u64 {
        return Err(PersistenceError::InvalidMorphPair(format!(
            "VMI declares {declared_count} deltas, but the VMB header declares {raw_entry_count}"
        )));
    }
    decode_vmb_daz_cm(vmb_bytes)
        .map_err(|error| PersistenceError::InvalidMorphPair(error.to_string()))?;
    Ok(())
}

fn private_pair_path(destination: &Path, sequence: u64, role: &str) -> PathBuf {
    let name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("vkit-vam-morph");
    destination.with_file_name(format!(
        ".{name}.{}.{}.{}",
        std::process::id(),
        sequence,
        role
    ))
}

fn write_synced_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.flush()?;
    file.sync_all()
}

struct MorphPairFile<'a> {
    path: &'a Path,
    backup: &'a Path,

    existed: bool,

    installed: bool,
}

fn rollback_morph_pair(vmi: MorphPairFile<'_>, vmb: MorphPairFile<'_>) -> io::Result<()> {
    for file in [&vmb, &vmi] {
        if file.installed && file.path.is_file() {
            fs::remove_file(file.path)?;
        }
    }
    for file in [&vmi, &vmb] {
        if file.existed {
            fs::rename(file.backup, file.path)?;
        }
    }
    Ok(())
}

struct PairStageCleanup(Vec<PathBuf>);

impl Drop for PairStageCleanup {
    fn drop(&mut self) {
        for path in self.0.drain(..) {
            let _ = fs::remove_file(path);
        }
    }
}

fn atomic_write(
    destination: &Path,
    write: impl FnOnce(&mut File) -> Result<(), PersistenceError>,
) -> Result<(), PersistenceError> {
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| PersistenceError::MissingParent(destination.to_path_buf()))?;
    if !parent.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("destination directory does not exist: {}", parent.display()),
        )
        .into());
    }

    let file_name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("vkit-output");
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        sequence
    ));
    let mut cleanup = TempCleanup(Some(temporary.clone()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    write(&mut file)?;
    file.flush()?;
    file.sync_all()?;
    drop(file);

    fs::rename(&temporary, destination)?;
    cleanup.0 = None;
    Ok(())
}

struct TempCleanup(Option<PathBuf>);

impl Drop for TempCleanup {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        let root = env::temp_dir().join(format!(
            "vkit-{label}-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn preferences_round_trip_outside_the_distribution() {
        let root = temp_root("preferences");
        let store = PreferenceStore::at(root.join("settings.json"));
        let expected = Preferences {
            shortcuts: std::collections::BTreeMap::new(),
            package_creator: Some("Some One".to_owned()),
            package_version: Some("3".to_owned()),
            package_license: Some("CC BY-NC".to_owned()),
            package_promotional_link: Some("https://example.invalid".to_owned()),
            locale: Some(Locale::Japanese),
            morph_name_display: MorphNameDisplay::Original,
            hair_toolbox_columns: 2,
            hair_toolbox_pos: Some([120.0, 240.0]),
            inspector_width: Some(512),
            vam_root: Some(PathBuf::from(r"C:\VaM")),
            vam_geometry_base_path: Some(PathBuf::from(r"C:\bases\femalecustom.obj")),
            figure_sex: Some(FigureSex::Male),
            vam_export_display_name: Some("Artist Smile".to_owned()),
            vam_export_group: Some("Expressions".to_owned()),
            vam_export_region: Some("Face/Mouth".to_owned()),
            vam_export_is_pose_control: Some(false),
            vam_export_bone_correction: Some(false),
            custom_head_solid_color_rgb: [0x73, 0xa7, 0xd7],
            g2_solid_color_rgb: [0xe8, 0xb2, 0x78],
            wireframe_color_rgb: [0x21, 0x43, 0x65],
            base_view_mode: BaseViewMode::Texture,
            surface_smooth_passes: 4,
            tooltips_enabled: true,
            show_one_sided_morphs: false,
            last_skin_id: None,
            default_skin_id: None,
            viewport_background_mode: ViewportBackgroundMode::Flat,
            wireframe_visible: true,
            wireframe_opacity: 0.61,
            xray_visible: true,
            xray_opacity: 0.42,
            scan_overlay: true,
            overlay_opacity: 0.37,
            show_result_tear_lacrimals: false,
            show_result_eyelashes: false,
            alignment_opacity: 0.72,
            alignment_g2_opacity: 0.64,
            light_yaw_radians: 1.25,
            lighting_preset: LightingPreset::Gloss,
            light_brightness: 1.45,
            tone_mapping: ToneMapping::default().id(),
            vignette_enabled: VignetteSettings::default().enabled,
            vignette_intensity: default_vignette_intensity(),
            vignette_smoothness: default_vignette_smoothness(),
            vignette_roundness: default_vignette_roundness(),
        };
        store.save(&expected).unwrap();
        assert_eq!(store.load().unwrap(), expected);
        assert!(!root.join("settings.json.tmp").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_preferences_ignore_removed_workspace_paths_and_manual_vam_id() {
        let legacy = br#"{"template_path":"G2.obj","output_path":"C:\\output\\head.obj","vam_export_id":"manual-id"}"#;
        let preferences: Preferences = serde_json::from_slice(legacy).unwrap();
        assert_eq!(preferences.inspector_width, None);
        assert_eq!(preferences.figure_sex, None);
        assert_eq!(preferences.locale, None);
        assert_eq!(preferences.vam_geometry_base_path, None);
        assert_eq!(preferences.base_view_mode, BaseViewMode::Solid);
        assert_eq!(preferences.surface_smooth_passes, VAM_SMOOTH_PASSES_DEFAULT);
        assert_eq!(
            preferences.viewport_background_mode,
            ViewportBackgroundMode::Radial
        );
        assert_eq!(preferences.wireframe_color_rgb, DEFAULT_WIREFRAME_COLOR_RGB);

        let serialized = serde_json::to_value(preferences).unwrap();
        assert!(serialized.get("output_path").is_none());
        assert!(serialized.get("vam_export_id").is_none());
        assert!(serialized.get("template_path").is_none());
    }

    #[test]
    fn persisted_view_preferences_sanitize_invalid_values_without_losing_legacy_settings() {
        let root = temp_root("preferences-sanitize");
        let path = root.join("settings.json");
        fs::write(
            &path,
            br#"{
                "template_path":"G2.obj",
                "base_view_mode":"unsupported",
                "surface_smooth_passes":99,
                "viewport_background_mode":" vertical ",
                "wireframe_color_rgb":[-2,127.6,999],
                "wireframe_opacity":-1,
                "xray_opacity":4,
                "overlay_opacity":0.25,
                "alignment_opacity":3,
                "alignment_g2_opacity":-3,
                "light_yaw_radians":7,
                "lighting_preset":"WARM",
                "light_brightness":8
            }"#,
        )
        .unwrap();

        let preferences = PreferenceStore::at(path).load().unwrap();
        assert_eq!(preferences.base_view_mode, BaseViewMode::Solid);
        assert_eq!(preferences.surface_smooth_passes, VAM_SMOOTH_PASSES_MAX);
        assert_eq!(
            preferences.viewport_background_mode,
            ViewportBackgroundMode::Vertical
        );
        assert_eq!(preferences.wireframe_color_rgb, [0, 128, 255]);
        assert_eq!(preferences.wireframe_opacity, 0.0);
        assert_eq!(preferences.xray_opacity, 1.0);
        assert_eq!(preferences.overlay_opacity, 0.25);
        assert_eq!(preferences.alignment_opacity, 1.0);
        assert_eq!(preferences.alignment_g2_opacity, 0.0);
        assert!((preferences.light_yaw_radians - (7.0_f32 % std::f32::consts::TAU)).abs() < 1e-6);
        assert_eq!(preferences.lighting_preset, LightingPreset::Warm);
        assert_eq!(preferences.light_brightness, 2.0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_settings_file_from_before_the_morph_name_switch_defaults_to_translated() {
        let legacy: Preferences =
            serde_json::from_str(r#"{"locale":"korean","inspector_width":400}"#)
                .expect("a file with no morph-name field still loads");
        assert_eq!(legacy.morph_name_display, MorphNameDisplay::Localized);
        assert_eq!(legacy.locale, Some(Locale::Korean));

        let garbled: Preferences =
            serde_json::from_str(r#"{"morph_name_display":"shouting","inspector_width":400}"#)
                .unwrap_or_default();
        assert_eq!(garbled.morph_name_display, MorphNameDisplay::Localized);
    }

    #[test]
    fn locale_migration_keeps_explicit_choices_and_defaults_the_rest() {
        let legacy: Preferences = serde_json::from_slice(br#"{"wireframe_visible":true}"#).unwrap();
        assert_eq!(legacy.locale, None);

        for (raw, expected) in [
            (r#""ko""#, Locale::Korean),
            (r#""en""#, Locale::English),
            (r#""ja""#, Locale::Japanese),
            (r#"" JA ""#, Locale::Japanese),
            (r#""Japanese""#, Locale::Japanese),
            (r#""english""#, Locale::English),
            (r#""not-a-locale""#, Locale::Korean),
        ] {
            let json = format!(r#"{{"locale":{raw}}}"#);
            let preferences: Preferences = serde_json::from_str(&json).unwrap();
            assert_eq!(preferences.locale, Some(expected), "{raw}");
        }

        let preferences = Preferences {
            locale: Some(Locale::Japanese),
            ..Preferences::default()
        };
        let serialized = serde_json::to_value(preferences).unwrap();
        assert_eq!(serialized.get("locale"), Some(&serde_json::json!("ja")));
    }

    #[test]
    fn preferences_have_no_runtime_only_reference_or_import_fields() {
        let serialized = serde_json::to_value(Preferences::default()).unwrap();
        for key in [
            "scan_import_completion",
            "import_progress",
            "scan_path",
            "template_path",
            "active_tab",
            "viewport_tool_panel",
            "scan_camera",
            "template_camera",
            "pins",
            "sculpt",
        ] {
            assert!(
                serialized.get(key).is_none(),
                "{key} must remain runtime-only"
            );
        }
    }

    fn valid_vam_pair_bytes(delta_y: f64) -> (Vec<u8>, Vec<u8>) {
        let metadata = vkit_core::vam::build_shape_vmi("test", "Test", 1);
        let vmi = vkit_core::vam::encode_vmi_pretty(&metadata).unwrap();
        let vmb = vkit_core::vam::encode_vmb_daz_cm(
            &[vkit_core::vam::SparseDelta {
                vertex_index: 1,
                delta_cm: [0.0, delta_y, 0.0],
            }],
            3,
        )
        .unwrap();
        (vmi, vmb)
    }

    #[test]
    fn vam_pair_commit_replaces_both_existing_siblings() {
        let root = temp_root("vam-pair-replace");
        let vmi_path = root.join("shape.vmi");
        let vmb_path = root.join("shape.vmb");
        let (first_vmi, first_vmb) = valid_vam_pair_bytes(0.25);
        write_vam_morph_pair_atomic(&vmi_path, &first_vmi, &vmb_path, &first_vmb).unwrap();
        let (second_vmi, second_vmb) = valid_vam_pair_bytes(0.75);
        write_vam_morph_pair_atomic(&vmi_path, &second_vmi, &vmb_path, &second_vmb).unwrap();

        assert_eq!(fs::read(&vmi_path).unwrap(), second_vmi);
        assert_eq!(fs::read(&vmb_path).unwrap(), second_vmb);
        let decoded = vkit_core::vam::decode_vmb_daz_cm(&second_vmb).unwrap();
        assert!((decoded[0].delta_cm[1] - 0.75).abs() < 1.0e-6);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn vam_pair_second_promotion_failure_restores_the_first_sibling() {
        let root = temp_root("vam-pair-rollback");
        let vmi_path = root.join("shape.vmi");
        let vmb_path = root.join("shape.vmb");
        fs::write(&vmi_path, b"previous-vmi").unwrap();
        fs::create_dir(&vmb_path).unwrap();
        let (vmi, vmb) = valid_vam_pair_bytes(0.5);

        assert!(write_vam_morph_pair_atomic(&vmi_path, &vmi, &vmb_path, &vmb).is_err());
        assert_eq!(fs::read(&vmi_path).unwrap(), b"previous-vmi");
        assert!(vmb_path.is_dir());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        fs::remove_dir_all(root).unwrap();
    }
}
