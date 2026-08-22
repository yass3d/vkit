#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![deny(unsafe_code)]
pub const APP_NAME: &str = "Vkit";

pub const VAM_TARGET_VERSION: &str = "1.22.0.13";

pub const APP_TITLE: &str = concat!("Vkit V", env!("CARGO_PKG_VERSION"));

pub const REPOSITORY_URL: &str = "https://github.com/yass3d/vkit";

pub const SUPPORT_URL: &str = "https://ko-fi.com/yass_3d";

mod appearance_layer_ui;
mod appearance_layers;
mod boot_window;
mod cache_paths;
mod camera;
mod camera_control;
mod cli;
mod diagnostics;
mod dialogs;
mod edit_clock;
mod guidance;
mod hair_collision;
mod hair_export;
mod hair_physics;
mod hair_portrait;
mod hair_preview;
mod hair_project;
mod hair_renderer;
mod hair_settings;
mod history;
mod i18n;
mod importers;
mod lighting;
mod logo_art;
mod look_head_filter;
mod morph_mask;
mod morphs;
mod persistence;
mod post_process;
mod recovery;
mod renderer;
mod responsive;
mod runtime;
mod scene;
mod sculpt;
mod session_snapshot;
mod settings;
mod shader_color;
mod shader_scene;
mod shortcuts;
mod skin_preview;
mod state;
mod svg_icon;
mod sweep_gesture;
mod texture_project;
mod texture_ui;
mod theme;
mod thumbnail;
mod ui;
mod ui_components;
mod unity_random;
mod update_check;
mod vam_catalog;
mod vam_edit_sources;
mod vam_hair;
mod vam_morph_cache;
mod vam_morph_index;
mod vam_skin;
mod viewport;
mod viewport_chrome;
mod viewport_tool_layout;
mod window_control;
mod workflow;

fn main() {
    let _ = diagnostics::initialize_global(env!("CARGO_PKG_VERSION"));
    let _ = diagnostics::install_panic_hook();
    if let Some(code) = cli::run_if_asked() {
        std::process::exit(code);
    }
    settle_cache_generation();
    if let Err(error) = runtime::run() {
        boot_window::report_startup_failure(error.as_ref());
    }
}

/// A cache written by another build is emptied before anything reads it.
fn settle_cache_generation() {
    let Some(root) = vkit_core::cache_root() else {
        return;
    };
    let version = env!("CARGO_PKG_VERSION");
    let (severity, message) = match vkit_core::cache::ensure_cache_generation(&root, version) {
        Ok(vkit_core::cache::CacheOutcome::Fresh) => {
            (diagnostics::Severity::Info, "cache created".to_owned())
        }
        Ok(vkit_core::cache::CacheOutcome::Kept) => (
            diagnostics::Severity::Info,
            format!(
                "cache kept at generation {}",
                vkit_core::cache::CACHE_GENERATION
            ),
        ),
        Ok(vkit_core::cache::CacheOutcome::Reset) => (
            diagnostics::Severity::Warning,
            format!(
                "cache from another generation emptied; rebuilding at {}",
                vkit_core::cache::CACHE_GENERATION
            ),
        ),
        Err(error) => (
            diagnostics::Severity::Error,
            format!("cache could not be brought up to date: {error}"),
        ),
    };
    let _ = diagnostics::record(severity, "cache", "generation", &message);
}
