#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![deny(unsafe_code)]
pub const APP_NAME: &str = "Vkit";

pub const VAM_TARGET_VERSION: &str = "1.22.0.13";

pub const APP_TITLE: &str = concat!("Vkit V", env!("CARGO_PKG_VERSION"));

pub const REPOSITORY_URL: &str = "https://github.com/yass3d/vkit";

pub const SUPPORT_URL: &str = "https://ko-fi.com/yass_3d";

mod ambient_occlusion;
mod bloom;
mod boot_window;
mod cache_paths;
mod camera;
mod camera_control;
mod diagnostics;
mod dialogs;
mod edit_clock;
mod guidance;
mod hair_physics;
mod hair_preview;
mod hair_renderer;
mod hdr_target;
mod i18n;
mod importers;
mod lighting;
mod logo_art;
mod look_head_filter;
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
mod ui;
mod ui_components;
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
    if let Err(error) = runtime::run() {
        boot_window::report_startup_failure(error.as_ref());
    }
}
