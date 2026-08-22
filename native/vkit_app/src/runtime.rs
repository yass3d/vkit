use std::{
    error::Error,
    ffi::OsString,
    num::NonZeroU32,
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver, TryRecvError},
    },
    thread,
    time::{Duration, Instant},
};

use egui::{Context, ViewportCommand, ViewportId};
use egui_wgpu::{RendererOptions, WgpuConfiguration, WgpuSetupCreateNew, winit::Painter};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowAttributes, WindowId},
};

use crate::{
    boot_window::{self, Desktop, MIN_HEIGHT, MIN_WIDTH, StartupPhase},
    diagnostics::{self, Severity},
    dialogs,
    i18n::Locale,
    importers::MeshImportProgress,
    persistence::{PreferenceStore, Preferences},
    renderer,
    scene::PreparedScan,
    state::{
        Action, AppState, ExportOutcome, GenerationOutcome, JobStage, VaMCatalogStatus,
        VarMetadataField, WorkspaceLoadJob, WorkspaceLoadOutcome,
    },
    texture_project::{TextureProjectCoordinator, TextureWorkerEvent},
    theme, ui,
    vam_catalog::{VaMCoordinator, VaMWorkerEvent},
    vam_skin::{SkinPreviewCoordinator, SkinPreviewEvent},
    workflow::{
        ExportCoordinator, ExportWorkerEvent, JobCoordinator, WorkerEvent,
        export_snapshot_from_state, snapshot_from_state,
    },
};

mod workers;

use workers::{ScanImportCoordinator, WorkspaceLoadCoordinator};

const CLEAR_COLOR: [f32; 4] = [
    crate::theme::COLOR_BG.r() as f32 / 255.0,
    crate::theme::COLOR_BG.g() as f32 / 255.0,
    crate::theme::COLOR_BG.b() as f32 / 255.0,
    1.0,
];
const SCAN_IMPORT_PROGRESS_INTERVAL: Duration = Duration::from_millis(33);
#[cfg(target_os = "windows")]
const WINDOWS_APP_USER_MODEL_ID: &str = "Vkit.Vkit";

#[cfg(target_os = "windows")]
struct StartupSurface {
    hwnd: windows_sys::Win32::Foundation::HWND,

    locale: Locale,
}

#[cfg(target_os = "windows")]
fn summarise_catalog_warnings(warnings: &[String]) -> String {
    use std::collections::BTreeMap;

    let mut by_reason: BTreeMap<&str, (usize, Vec<&str>)> = BTreeMap::new();
    for warning in warnings {
        let reason = warning.rsplit(": ").next().unwrap_or(warning.as_str());
        let package = warning
            .split(['/', '\\'])
            .next_back()
            .and_then(|tail| tail.split(':').next())
            .unwrap_or("?");
        let entry = by_reason.entry(reason).or_default();
        entry.0 += 1;
        if entry.1.len() < 2 {
            entry.1.push(package);
        }
    }
    let mut out = format!("count={}", warnings.len());
    for (reason, (count, examples)) in by_reason {
        out.push_str(&format!("; {count}x {reason} ({})", examples.join(", ")));
    }
    out
}

impl StartupSurface {
    fn new(window: &Window, locale: Locale) -> Option<Self> {
        use winit::raw_window_handle::{HasWindowHandle as _, RawWindowHandle};

        let handle = window.window_handle().ok()?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return None;
        };
        let hwnd = handle.hwnd.get() as windows_sys::Win32::Foundation::HWND;
        (!hwnd.is_null()).then_some(Self { hwnd, locale })
    }

    #[allow(unsafe_code, reason = "isolated Win32 GDI bootstrap surface")]
    fn paint(&self, phase: StartupPhase) {
        use windows_sys::Win32::{
            Foundation::RECT,
            Graphics::Gdi::{
                CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, CreateSolidBrush,
                DEFAULT_CHARSET, DEFAULT_PITCH, DT_CENTER, DT_SINGLELINE, DT_VCENTER, DeleteObject,
                DrawTextW, FW_SEMIBOLD, FillRect, GdiFlush, GetDC, OUT_DEFAULT_PRECIS, ReleaseDC,
                SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
            },
            UI::WindowsAndMessaging::GetClientRect,
        };

        const fn color_ref(color: egui::Color32) -> u32 {
            color.r() as u32 | ((color.g() as u32) << 8) | ((color.b() as u32) << 16)
        }

        fn wide(value: &str) -> Vec<u16> {
            value.encode_utf16().collect()
        }

        unsafe {
            let hdc = GetDC(self.hwnd);
            if hdc.is_null() {
                return;
            }
            let mut client: RECT = std::mem::zeroed();
            if GetClientRect(self.hwnd, &mut client) == 0 {
                ReleaseDC(self.hwnd, hdc);
                return;
            }

            let background = CreateSolidBrush(color_ref(crate::theme::COLOR_BG));
            if !background.is_null() {
                FillRect(hdc, &client, background);
                DeleteObject(background);
            }
            SetBkMode(hdc, TRANSPARENT as i32);

            let center_y = (client.top + client.bottom) / 2;

            let font_face: Vec<u16> = crate::boot_window::boot_font_face(self.locale)
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let title_font = CreateFontW(
                -28,
                0,
                0,
                0,
                FW_SEMIBOLD as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                DEFAULT_PITCH as u32,
                font_face.as_ptr(),
            );
            let previous_font = if title_font.is_null() {
                std::ptr::null_mut()
            } else {
                SelectObject(hdc, title_font)
            };
            SetTextColor(hdc, color_ref(crate::theme::COLOR_TEXT));
            let title = wide(crate::APP_TITLE);
            let mut title_rect = RECT {
                left: client.left,
                top: center_y - 64,
                right: client.right,
                bottom: center_y - 24,
            };
            DrawTextW(
                hdc,
                title.as_ptr(),
                title.len() as i32,
                &mut title_rect,
                DT_CENTER | DT_SINGLELINE | DT_VCENTER,
            );
            if !previous_font.is_null() {
                SelectObject(hdc, previous_font);
            }
            if !title_font.is_null() {
                DeleteObject(title_font);
            }

            let body_font = CreateFontW(
                -15,
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                DEFAULT_PITCH as u32,
                font_face.as_ptr(),
            );
            let previous_font = if body_font.is_null() {
                std::ptr::null_mut()
            } else {
                SelectObject(hdc, body_font)
            };
            SetTextColor(hdc, color_ref(crate::theme::COLOR_MUTED));
            let label = wide(phase.label(self.locale));
            let mut label_rect = RECT {
                left: client.left,
                top: center_y - 10,
                right: client.right,
                bottom: center_y + 30,
            };
            DrawTextW(
                hdc,
                label.as_ptr(),
                label.len() as i32,
                &mut label_rect,
                DT_CENTER | DT_SINGLELINE | DT_VCENTER,
            );
            if !previous_font.is_null() {
                SelectObject(hdc, previous_font);
            }
            if !body_font.is_null() {
                DeleteObject(body_font);
            }

            let available = (client.right - client.left - 48).max(120);
            let track_width = available.min(360);
            let track_left = client.left + (client.right - client.left - track_width) / 2;
            let track = RECT {
                left: track_left,
                top: center_y + 50,
                right: track_left + track_width,
                bottom: center_y + 53,
            };
            let track_brush = CreateSolidBrush(color_ref(crate::theme::COLOR_TRACK));
            if !track_brush.is_null() {
                FillRect(hdc, &track, track_brush);
                DeleteObject(track_brush);
            }
            let filled = ((track_width as f32) * phase.fraction()).round() as i32;
            let fill = RECT {
                right: track.left + filled.clamp(0, track_width),
                ..track
            };
            let progress_brush = CreateSolidBrush(color_ref(crate::theme::COLOR_TRACK_FILL));
            if !progress_brush.is_null() {
                FillRect(hdc, &fill, progress_brush);
                DeleteObject(progress_brush);
            }
            GdiFlush();
            ReleaseDC(self.hwnd, hdc);
        }
    }
}

#[cfg(not(target_os = "windows"))]
struct StartupSurface;

#[cfg(not(target_os = "windows"))]
impl StartupSurface {
    fn new(_window: &Window, _locale: Locale) -> Option<Self> {
        Some(Self)
    }

    fn paint(&self, _phase: StartupPhase) {}
}

fn log(severity: Severity, event: &str, message: &str) {
    let _ = diagnostics::record(severity, "runtime", event, message);
}

fn log_font_report(report: &theme::FontReport, locale: Locale) {
    if report.fonts.is_empty() {
        log(
            Severity::Warning,
            "font_fallback",
            "no Windows UI font could be loaded",
        );
        return;
    }
    let chain = report
        .fonts
        .iter()
        .map(|font| {
            if font.y_offset_factor == 0.0 {
                font.path.display().to_string()
            } else {
                format!(
                    "{} (y_offset_factor={:+.3})",
                    font.path.display(),
                    font.y_offset_factor
                )
            }
        })
        .collect::<Vec<_>>()
        .join(" -> ");
    log(
        Severity::Info,
        "font_loaded",
        &format!("locale={locale:?}; chain={chain}"),
    );
    if !report.korean_ready {
        log(
            Severity::Warning,
            "font_fallback",
            "Korean system font could not be loaded",
        );
    }
    if !report.locale_ready {
        log(
            Severity::Warning,
            "font_locale_fallback",
            &format!("no primary-script font for {locale:?} could be loaded"),
        );
    }
}

pub fn run() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::<RuntimeEvent>::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut application = NativeApplication {
        runtime: None,
        next_repaint: None,
        proxy: event_loop.create_proxy(),
    };
    event_loop.run_app(&mut application)?;
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum RuntimeEvent {
    WorkerWake,
}

enum ScanImportEvent {
    Progress {
        path: PathBuf,
        progress: MeshImportProgress,
    },
    Finished {
        path: PathBuf,
        outcome: Result<PreparedScan, String>,
    },
}

struct NativeApplication {
    runtime: Option<Runtime>,
    next_repaint: Option<Instant>,
    proxy: EventLoopProxy<RuntimeEvent>,
}

fn dropped_file_action(path: &std::path::Path) -> Option<Action> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "dsf" => Some(Action::LoadTemplate(path.to_path_buf())),
        "obj" | "glb" | "gltf" | "fbx" => Some(Action::LoadScan(path.to_path_buf())),
        _ => None,
    }
}

impl ApplicationHandler<RuntimeEvent> for NativeApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.runtime.is_some() {
            return;
        }

        match Runtime::create(event_loop, self.proxy.clone()) {
            Ok(runtime) => {
                runtime.window.request_redraw();
                self.runtime = Some(runtime);
            }
            Err(error) => {
                boot_window::report_startup_failure(error.as_ref());
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        if window_id != runtime.window.id() {
            return;
        }

        if let WindowEvent::KeyboardInput { event: key, .. } = &event
            && claims_numpad(key, runtime.context.egui_wants_keyboard_input())
        {
            if let Some(shortcut) =
                runtime_shortcut_for_physical_key(key.physical_key, key.state, key.repeat, false)
            {
                runtime.state.dispatch(shortcut.into_action());
            }
            runtime.window.request_redraw();
            return;
        }

        let response = runtime
            .egui_state
            .on_window_event(runtime.window.as_ref(), &event);
        if should_schedule_event_repaint(&event, response.repaint) {
            runtime.window.request_redraw();
        }

        match event {
            WindowEvent::CloseRequested => {
                runtime.close(event_loop);
            }
            WindowEvent::Resized(size) => {
                runtime.resize(size.width, size.height);
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                runtime.window.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let keyboard_captured = runtime.context.egui_wants_keyboard_input();
                if let Some(shortcut) = runtime_shortcut_for_physical_key(
                    event.physical_key,
                    event.state,
                    event.repeat,
                    keyboard_captured,
                ) {
                    runtime.state.dispatch(shortcut.into_action());
                    runtime.window.request_redraw();
                }
            }
            WindowEvent::DroppedFile(path) => {
                if let Some(action) = dropped_file_action(&path) {
                    runtime.state.dispatch(action);
                    runtime.persist_preferences();
                }
                runtime.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                self.next_repaint = runtime.render(event_loop);
            }
            _ => {}
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: RuntimeEvent) {
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        match event {
            RuntimeEvent::WorkerWake => runtime.pump_lanes(),
        }
        runtime.window.request_redraw();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(runtime) = self.runtime.as_ref() else {
            return;
        };

        match self.next_repaint {
            Some(deadline) if deadline <= Instant::now() => {
                self.next_repaint = None;
                runtime.window.request_redraw();
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            Some(deadline) => event_loop.set_control_flow(ControlFlow::WaitUntil(deadline)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }
}

const POINTER_RELEASE_LOG_EVERY: u32 = 25;

const fn should_log_pointer_release(occurrence: u32) -> bool {
    occurrence == 1 || occurrence.is_multiple_of(POINTER_RELEASE_LOG_EVERY)
}

fn should_log_job_progress(
    last_logged: Option<(u64, JobStage, f32)>,
    job_id: u64,
    stage: JobStage,
    fraction: f32,
) -> bool {
    let whole_percent = |fraction: f32| (fraction * 100.0).floor();
    match last_logged {
        None => true,
        Some((last_job, last_stage, last_fraction)) => {
            last_job != job_id
                || last_stage != stage
                || whole_percent(fraction) != whole_percent(last_fraction)
                || (fraction >= 1.0 && last_fraction < 1.0)
        }
    }
}

fn elapsed_ms(started: Option<Instant>) -> f64 {
    started.map_or(0.0, |at| at.elapsed().as_secs_f64() * 1000.0)
}

fn disarm_quit_shortcuts(context: &Context) {
    context.options_mut(|options| options.quit_shortcuts.clear());
}

fn directory_bytes(path: &std::path::Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => directory_bytes(&entry.path()),
            Ok(_) => entry.metadata().map_or(0, |metadata| metadata.len()),
            Err(_) => 0,
        })
        .sum()
}

fn empty_directory(path: &std::path::Path) -> usize {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => std::fs::remove_dir_all(entry.path()).is_ok(),
            Ok(_) => std::fs::remove_file(entry.path()).is_ok(),
            Err(_) => false,
        })
        .count()
}

fn relaunch() {
    let Ok(executable) = std::env::current_exe() else {
        return;
    };
    match std::process::Command::new(&executable).spawn() {
        Ok(_) => log(
            Severity::Info,
            "restarting",
            &executable.display().to_string(),
        ),
        Err(error) => log(Severity::Warning, "restart_failed", &error.to_string()),
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct FrameTrace {
    frames: u32,
    ui_ms: f64,
    tessellate_ms: f64,
    paint_ms: f64,
    worst_total_ms: f64,
}

impl FrameTrace {
    const REPORT_EVERY: u32 = 60;

    fn record(&mut self, ui_ms: f64, tessellate_ms: f64, paint_ms: f64) {
        self.frames += 1;
        self.ui_ms += ui_ms;
        self.tessellate_ms += tessellate_ms;
        self.paint_ms += paint_ms;
        self.worst_total_ms = self.worst_total_ms.max(ui_ms + tessellate_ms + paint_ms);
        if self.frames < Self::REPORT_EVERY {
            return;
        }
        let frames = f64::from(self.frames);
        log(
            Severity::Debug,
            "frame_timing",
            &format!(
                "frames={}; mean_total_ms={:.2}; ui_ms={:.2}; tessellate_ms={:.2}; paint_ms={:.2}; worst_total_ms={:.2}",
                self.frames,
                (self.ui_ms + self.tessellate_ms + self.paint_ms) / frames,
                self.ui_ms / frames,
                self.tessellate_ms / frames,
                self.paint_ms / frames,
                self.worst_total_ms,
            ),
        );
        *self = Self::default();
    }
}

fn should_schedule_event_repaint(event: &WindowEvent, repaint_requested: bool) -> bool {
    if !repaint_requested {
        return false;
    }
    !matches!(event, WindowEvent::RedrawRequested | WindowEvent::Moved(_))
}

fn native_window_attributes(desktop: Option<Desktop>) -> WindowAttributes {
    let (width, height) = desktop.map_or(
        (boot_window::PREFERRED_WIDTH, boot_window::PREFERRED_HEIGHT),
        Desktop::opening_points,
    );
    Window::default_attributes()
        .with_title(crate::APP_TITLE)
        .with_decorations(false)
        .with_inner_size(LogicalSize::new(width, height))
        .with_min_inner_size(LogicalSize::new(MIN_WIDTH, MIN_HEIGHT))
}

#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "isolated Win32 process identity binding before HWND creation"
)]
fn set_windows_app_user_model_id() -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

    let app_id = WINDOWS_APP_USER_MODEL_ID
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();

    let result = unsafe { SetCurrentProcessExplicitAppUserModelID(app_id.as_ptr()) };
    if result >= 0 {
        Ok(())
    } else {
        Err(format!(
            "SetCurrentProcessExplicitAppUserModelID returned HRESULT 0x{result:08X}"
        ))
    }
}

#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "isolated post-creation Win32 icon binding for the live HWND"
)]
fn bind_windows_window_icons(window: &Window) -> Result<(), String> {
    use windows_sys::Win32::{
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            GetSystemMetrics, ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_SHARED, LoadImageW, SM_CXICON,
            SM_CXSMICON, SM_CYICON, SM_CYSMICON, SendMessageW, WM_SETICON,
        },
    };

    let hwnd = crate::window_control::windows_hwnd(window)?;

    let module = unsafe { GetModuleHandleW(std::ptr::null()) };
    if module.is_null() {
        return Err("GetModuleHandleW returned a null module".to_owned());
    }

    let resource_name = std::ptr::without_provenance::<u16>(1);

    let big_icon = unsafe {
        LoadImageW(
            module,
            resource_name,
            IMAGE_ICON,
            GetSystemMetrics(SM_CXICON),
            GetSystemMetrics(SM_CYICON),
            LR_SHARED,
        )
    };
    if big_icon.is_null() {
        return Err("LoadImageW returned a null large icon for resource 1".to_owned());
    }

    let small_icon = unsafe {
        LoadImageW(
            module,
            resource_name,
            IMAGE_ICON,
            GetSystemMetrics(SM_CXSMICON),
            GetSystemMetrics(SM_CYSMICON),
            LR_SHARED,
        )
    };
    if small_icon.is_null() {
        return Err("LoadImageW returned a null small icon for resource 1".to_owned());
    }

    unsafe {
        SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, big_icon as isize);
        SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, small_icon as isize);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RuntimeShortcut {
    ResetCamera,
    ToggleProjection,
    StandardView(crate::camera::StandardView),
}

impl RuntimeShortcut {
    const fn into_action(self) -> Action {
        match self {
            Self::ResetCamera => Action::ResetCamera,
            Self::ToggleProjection => Action::ToggleProjection,
            Self::StandardView(view) => Action::SetStandardView(view),
        }
    }
}

fn claims_numpad(key: &winit::event::KeyEvent, keyboard_captured: bool) -> bool {
    !keyboard_captured && matches!(key.physical_key, PhysicalKey::Code(code) if is_numpad(code))
}

const fn is_numpad(code: KeyCode) -> bool {
    matches!(
        code,
        KeyCode::Numpad0
            | KeyCode::Numpad1
            | KeyCode::Numpad2
            | KeyCode::Numpad3
            | KeyCode::Numpad4
            | KeyCode::Numpad5
            | KeyCode::Numpad6
            | KeyCode::Numpad7
            | KeyCode::Numpad8
            | KeyCode::Numpad9
            | KeyCode::NumpadDecimal
    )
}

fn runtime_shortcut_for_physical_key(
    physical_key: PhysicalKey,
    state: ElementState,
    repeat: bool,
    keyboard_captured: bool,
) -> Option<RuntimeShortcut> {
    if keyboard_captured || repeat || state != ElementState::Pressed {
        return None;
    }

    match physical_key {
        PhysicalKey::Code(KeyCode::Home | KeyCode::Numpad0) => Some(RuntimeShortcut::ResetCamera),
        PhysicalKey::Code(KeyCode::NumpadDecimal) => Some(RuntimeShortcut::ToggleProjection),

        PhysicalKey::Code(KeyCode::Numpad5) => Some(RuntimeShortcut::StandardView(
            crate::camera::StandardView::Front,
        )),
        PhysicalKey::Code(KeyCode::Numpad4) => Some(RuntimeShortcut::StandardView(
            crate::camera::StandardView::LeftSide,
        )),
        PhysicalKey::Code(KeyCode::Numpad6) => Some(RuntimeShortcut::StandardView(
            crate::camera::StandardView::RightSide,
        )),
        PhysicalKey::Code(KeyCode::Numpad8) => Some(RuntimeShortcut::StandardView(
            crate::camera::StandardView::Top,
        )),
        PhysicalKey::Code(KeyCode::Numpad2) => Some(RuntimeShortcut::StandardView(
            crate::camera::StandardView::Bottom,
        )),
        PhysicalKey::Code(KeyCode::Numpad7) => Some(RuntimeShortcut::StandardView(
            crate::camera::StandardView::FrontUpperLeft,
        )),
        PhysicalKey::Code(KeyCode::Numpad9) => Some(RuntimeShortcut::StandardView(
            crate::camera::StandardView::FrontUpperRight,
        )),
        PhysicalKey::Code(KeyCode::Numpad1) => Some(RuntimeShortcut::StandardView(
            crate::camera::StandardView::FrontLowerLeft,
        )),
        PhysicalKey::Code(KeyCode::Numpad3) => Some(RuntimeShortcut::StandardView(
            crate::camera::StandardView::FrontLowerRight,
        )),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "isolated best-effort Windows 11 DWM window preference"
)]
fn prefer_windows_rounded_corners(window: &Window) -> Result<(), String> {
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
    };
    let hwnd = crate::window_control::windows_hwnd(window)?;

    let preference = DWMWCP_ROUND;

    let result = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            (&preference as *const i32).cast(),
            std::mem::size_of_val(&preference) as u32,
        )
    };
    if result >= 0 {
        Ok(())
    } else {
        Err(format!(
            "DwmSetWindowAttribute returned HRESULT 0x{result:08X}"
        ))
    }
}

struct Runtime {
    window: Arc<Window>,
    context: Context,
    egui_state: egui_winit::State,
    painter: Painter,
    portrait_canvas: Option<crate::hair_portrait::PortraitTarget>,
    state: AppState,

    logged_status: Option<String>,
    scan_imports: ScanImportCoordinator,
    workspace_loads: WorkspaceLoadCoordinator,
    jobs: JobCoordinator,
    exports: ExportCoordinator,
    vam_appearance_assets: VaMCoordinator,
    builtin_morph_assets: VaMCoordinator,
    skin_assets: SkinPreviewCoordinator,
    texture_assets: TextureProjectCoordinator,
    proxy: EventLoopProxy<RuntimeEvent>,
    preferences: Option<PreferenceStore>,

    installed_font_locale: Locale,

    font_probe_pending: bool,
    hair_disturbance: (u64, bool, bool),
    last_morph_timing_serial: u64,
    morph_timing_log_count: u8,
    last_sculpt_timing_serial: u64,
    sculpt_timing_log_count: u8,
    repaint_trace_count: u8,

    texture_bake_started: Option<(u64, Instant)>,

    frame_trace: Option<FrameTrace>,

    pointer_release_count: u32,

    last_logged_job_progress: Option<(u64, JobStage, f32)>,

    recovery: Option<crate::recovery::RecoveryStore>,
    autosave: crate::recovery::AutosaveSchedule,

    session_started: Instant,

    snapshotted_edits: (u64, u64, u64),

    last_saved_preferences: Option<Preferences>,

    preferences_checked: std::time::Duration,
}

impl Runtime {
    fn create(
        event_loop: &ActiveEventLoop,
        proxy: EventLoopProxy<RuntimeEvent>,
    ) -> Result<Self, Box<dyn Error>> {
        #[cfg(target_os = "windows")]
        match set_windows_app_user_model_id() {
            Ok(()) => log(
                Severity::Info,
                "app_user_model_id_set",
                WINDOWS_APP_USER_MODEL_ID,
            ),
            Err(error) => log(Severity::Warning, "app_user_model_id_failed", &error),
        }

        let desktop = boot_window::primary_desktop(
            event_loop
                .primary_monitor()
                .map_or(1.0, |monitor| monitor.scale_factor()),
        );
        if let Some(desktop) = desktop
            && !desktop.fits_minimum_window()
        {
            let (width, height) = desktop.usable_points();
            log(
                Severity::Warning,
                "desktop_smaller_than_minimum_window",
                &format!(
                    "work area is {width:.0}x{height:.0} points; the window cannot go below \
                     {MIN_WIDTH:.0}x{MIN_HEIGHT:.0}"
                ),
            );
        }
        let mut attributes = native_window_attributes(desktop);
        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::{IconExtWindows, WindowAttributesExtWindows};

            match winit::window::Icon::from_resource(1, None) {
                Ok(icon) => {
                    attributes = attributes
                        .with_window_icon(Some(icon.clone()))
                        .with_taskbar_icon(Some(icon));
                    log(
                        Severity::Info,
                        "icon_loaded",
                        "embedded Windows icon resource 1",
                    );
                }
                Err(error) => log(Severity::Warning, "icon_load_failed", &error.to_string()),
            }
        }
        let window = Arc::new(event_loop.create_window(attributes)?);
        #[cfg(target_os = "windows")]
        match bind_windows_window_icons(window.as_ref()) {
            Ok(()) => log(
                Severity::Info,
                "window_icons_bound",
                "bound resource 1 as the live HWND large and small icon",
            ),
            Err(error) => log(Severity::Warning, "window_icon_bind_failed", &error),
        }
        #[cfg(target_os = "windows")]
        match prefer_windows_rounded_corners(window.as_ref()) {
            Ok(()) => log(
                Severity::Info,
                "rounded_corners_enabled",
                "requested the Windows 11 rounded-corner preference",
            ),
            Err(error) => log(
                Severity::Info,
                "rounded_corners_unavailable",
                &format!("best-effort DWM preference was not applied: {error}"),
            ),
        }
        #[cfg(target_os = "windows")]
        match crate::window_control::install_window_subclass(window.as_ref()) {
            Ok(()) => log(
                Severity::Info,
                "nc_hit_test_installed",
                "native non-client hit-testing owns titlebar drags and resizes",
            ),
            Err(error) => log(
                Severity::Warning,
                "nc_hit_test_unavailable",
                &format!("falling back to client-side drag emulation: {error}"),
            ),
        }

        let preferences = PreferenceStore::discover();
        let saved = preferences
            .as_ref()
            .and_then(|store| match store.load() {
                Ok(saved) => Some(saved),
                Err(error) => {
                    log(
                        Severity::Warning,
                        "settings_load_failed",
                        &error.to_string(),
                    );
                    None
                }
            })
            .unwrap_or_default();

        let startup_locale = boot_window::boot_locale(saved.locale);

        let startup_surface = StartupSurface::new(window.as_ref(), startup_locale);
        if let Some(surface) = startup_surface.as_ref() {
            surface.paint(StartupPhase::Window);
        }

        let context = Context::default();
        disarm_quit_shortcuts(&context);
        crate::update_check::start(&context);

        let recovery = crate::recovery::RecoveryStore::discover();
        let abandoned = recovery.as_ref().and_then(|store| {
            matches!(
                store.inspect(std::time::SystemTime::now()),
                crate::recovery::LockState::Stale
            )
            .then(|| store.load())
            .flatten()
        });
        let recovery = recovery.filter(|store| match store.claim() {
            Ok(()) => true,
            Err(error) => {
                log(
                    Severity::Warning,
                    "recovery_lock_failed",
                    &format!("autosave is off for this session: {error}"),
                );
                false
            }
        });
        let font_report = theme::configure_context(&context, startup_locale);
        log_font_report(&font_report, startup_locale);
        if let Some(surface) = startup_surface.as_ref() {
            surface.paint(StartupPhase::Font);
            surface.paint(StartupPhase::Gpu);
        }

        let mut setup = WgpuSetupCreateNew::from_display_handle(event_loop.owned_display_handle());
        setup.instance_descriptor.backends = wgpu::Backends::DX12;
        setup.instance_descriptor.flags = wgpu::InstanceFlags::empty().with_env();
        setup
            .instance_descriptor
            .backend_options
            .dx12
            .shader_compiler = wgpu::Dx12Compiler::Fxc;
        setup.power_preference = wgpu::PowerPreference::HighPerformance;

        setup.device_descriptor = std::sync::Arc::new(|adapter: &wgpu::Adapter| {
            let mut limits = wgpu::Limits::default();
            limits.max_sampled_textures_per_shader_stage = adapter
                .limits()
                .max_sampled_textures_per_shader_stage
                .max(limits.max_sampled_textures_per_shader_stage);
            wgpu::DeviceDescriptor {
                label: Some("vkit"),
                // Without this a device is guaranteed only 1x and 4x whatever the
                // adapter advertises, and asking for 2x or 8x is a texture it
                // refuses to create. Asked for only where it is offered, so an
                // adapter without it still starts.
                required_features: adapter
                    .features()
                    .intersection(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES),
                required_limits: limits,
                memory_hints: wgpu::MemoryHints::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                trace: wgpu::Trace::Off,
            }
        });

        // The sample count has to be settled before the painter is built: egui
        // bakes it into its own pipelines at construction and every pipeline in
        // this program has to agree with it. That is why the preference takes
        // effect on restart, and why it is probed against the adapter first —
        // a count the adapter cannot carry would fail validation on every pass
        // rather than degrade.
        let wanted_samples = saved.msaa_samples;
        let active_samples = probe_msaa_samples(wanted_samples);
        if active_samples != wanted_samples {
            log(
                Severity::Warning,
                "msaa_unsupported",
                &format!(
                    "this adapter cannot carry {wanted_samples}x, running at {active_samples}x"
                ),
            );
        }
        log(
            Severity::Info,
            "msaa_selected",
            &format!(
                "{active_samples}x of {:?} available",
                renderer::supported_msaa_samples()
            ),
        );

        let configuration = WgpuConfiguration {
            wgpu_setup: setup.into(),

            surface: egui_wgpu::SurfaceConfig::LOW_LATENCY,
            ..Default::default()
        };
        let renderer_options = RendererOptions {
            // egui's own pipelines, not the scene's. The scene draws on a
            // surface of its own at `active_samples`, which is why this can be
            // one and why the setting can change without a restart.
            msaa_samples: renderer::EGUI_MSAA_SAMPLES,
            depth_stencil_format: Some(renderer::DEPTH_FORMAT),
            ..Default::default()
        };

        let mut painter = pollster::block_on(Painter::new(
            context.clone(),
            configuration,
            false,
            renderer_options,
        ));

        if let Err(error) =
            pollster::block_on(painter.set_window(ViewportId::ROOT, Some(window.clone())))
        {
            return Err(Box::new(boot_window::GraphicsUnavailable::new(error)));
        }
        if let Err(error) = renderer::install(&painter) {
            log(
                Severity::Error,
                "renderer_install_failed",
                &error.to_string(),
            );
        }
        if let Some(surface) = startup_surface.as_ref() {
            surface.paint(StartupPhase::Preferences);
        }

        let egui_state = egui_winit::State::new(
            context.clone(),
            ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            window.theme(),
            painter.max_texture_side(),
        );

        let mut state = AppState::default();

        state.dispatch(Action::SetLocale(startup_locale));
        apply_saved_preferences(&mut state, &saved);
        let startup = parse_startup_paths(std::env::args_os());
        if let Some(path) = startup
            .output
            .or_else(|| std::env::var_os("VKIT_OUTPUT").map(PathBuf::from))
        {
            state.dispatch(Action::SetOutputPath(path.to_string_lossy().into_owned()));
        }
        if let Some(path) = startup.scan {
            state.dispatch(Action::LoadScan(path));
        }
        if let Some(surface) = startup_surface.as_ref() {
            surface.paint(StartupPhase::Template);
        }
        if let Some(path) = startup
            .template
            .or_else(|| std::env::var_os("VKIT_G2_DSF").map(PathBuf::from))
            .or_else(|| std::env::var_os("VKIT_G2_OBJ").map(PathBuf::from))
        {
            state.dispatch(Action::LoadTemplate(path));
        }
        if let Some(path) = startup
            .vam
            .or_else(|| std::env::var_os("VKIT_VAM_ROOT").map(PathBuf::from))
            .or(saved.vam_root)
        {
            state.dispatch(Action::SetVaMRoot(path));
        } else {
            state.flash_attention(crate::state::AttentionTarget::VaMRoot);
        }

        state.vam_geometry_base_path = saved.vam_geometry_base_path;
        if let Some(surface) = startup_surface.as_ref() {
            surface.paint(StartupPhase::Workspace);
        }
        if let Some(path) = startup.result {
            state.dispatch(Action::LoadResult(path));
        }

        if state.vam_root.is_some()
            && matches!(state.vam_catalog_status, VaMCatalogStatus::Unconfigured)
            && !state.template_load_active()
        {
            state.dispatch(Action::RefreshVaMCatalog);
            log(
                Severity::Info,
                "vam_catalog_prewarm",
                "queued the VaM appearance scan during startup",
            );
        }
        if let Some(snapshot) = abandoned {
            log(
                Severity::Info,
                "recovery_offer",
                "the last session did not release its lock; offering its snapshot",
            );
            state.pending_recovery = Some(snapshot);
        }
        if let Some(surface) = startup_surface.as_ref() {
            surface.paint(StartupPhase::Ready);
        }

        log(
            Severity::Info,
            "ready",
            "DX12 renderer and native UI initialized",
        );
        let snapshotted_edits = state.autosave_edit_signal();
        let locale_undecided = saved.locale.is_none();
        let mut runtime = Self {
            logged_status: None,
            window,
            context,
            egui_state,
            painter,
            portrait_canvas: None,
            state,
            scan_imports: ScanImportCoordinator::default(),
            workspace_loads: WorkspaceLoadCoordinator::default(),
            jobs: JobCoordinator::default(),
            exports: ExportCoordinator::default(),
            vam_appearance_assets: VaMCoordinator::default(),
            builtin_morph_assets: VaMCoordinator::default(),
            skin_assets: SkinPreviewCoordinator::default(),
            texture_assets: TextureProjectCoordinator::default(),
            proxy,
            preferences,
            installed_font_locale: startup_locale,
            font_probe_pending: true,
            hair_disturbance: (0, false, false),
            last_morph_timing_serial: 0,
            morph_timing_log_count: 0,
            last_sculpt_timing_serial: 0,
            sculpt_timing_log_count: 0,
            repaint_trace_count: 0,
            texture_bake_started: None,
            frame_trace: frame_trace(),
            pointer_release_count: 0,
            last_logged_job_progress: None,
            recovery,
            autosave: crate::recovery::AutosaveSchedule::default(),
            session_started: Instant::now(),
            snapshotted_edits,
            last_saved_preferences: None,
            preferences_checked: std::time::Duration::ZERO,
        };
        if locale_undecided {
            runtime.persist_preferences();
        }
        Ok(runtime)
    }

    fn resize(&mut self, width: u32, height: u32) {
        let (Some(width), Some(height)) = (NonZeroU32::new(width), NonZeroU32::new(height)) else {
            return;
        };
        self.painter
            .on_window_resized(ViewportId::ROOT, width, height);
    }

    fn pump_lanes(&mut self) {
        self.poll_worker();
        self.poll_scan_import();
        self.poll_workspace_load();
        self.poll_export();
        self.poll_vam_assets();
        self.poll_skin_assets();
        self.poll_texture_assets();
    }

    fn render(&mut self, event_loop: &ActiveEventLoop) -> Option<Instant> {
        self.pump_lanes();

        self.drain_synthetic_pointer_release();

        let disturbance = (
            self.state.hair_project.edit_revision,
            self.state.hair_viewport_physics,
            self.state.is_hair_editing(),
        );
        if disturbance != self.hair_disturbance {
            self.hair_disturbance = disturbance;
            self.state.hair_simulation_seconds = crate::state::HAIR_SIMULATION_SECONDS;
        }
        if self.state.hair_settle_seconds > 0.0 || self.state.hair_simulation_seconds > 0.0 {
            let elapsed = self.context.input(|input| input.stable_dt);
            self.state
                .dispatch(Action::SpendHairSettle(elapsed.clamp(0.0, 0.1)));
        }
        let mut input = self.egui_state.take_egui_input(self.window.as_ref());

        {
            let wants_keyboard = self.context.egui_wants_keyboard_input();
            let mut plain_tab_pressed = false;
            input.events.retain(|event| {
                if let egui::Event::Key {
                    key: egui::Key::Tab,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                    && !wants_keyboard
                    && (modifiers.is_none() || modifiers.shift_only())
                {
                    plain_tab_pressed |= modifiers.is_none();
                    return false;
                }
                true
            });
        }
        let ui_started = self.frame_trace.map(|_| Instant::now());
        theme::set_tooltips_enabled(&self.context, self.state.tooltips_enabled);
        let mut output = self.context.run_ui(input, |root| {
            let state = &mut self.state;
            let _ = catch_unwind(AssertUnwindSafe(|| ui::draw(root, state)));
        });
        let ui_ms = elapsed_ms(ui_started);
        self.paint_pending_hair_portraits();
        self.shoot_hair_thumbnail();
        self.record_new_status();
        self.tick_autosave();
        self.tick_preferences();
        self.tick_cache_maintenance();
        if self.state.settings_reset {
            self.close(event_loop);
            return None;
        }

        if self.font_probe_pending {
            self.font_probe_pending = false;
            self.log_glyph_probe();
        }
        if self.state.locale != self.installed_font_locale {
            let report = theme::install_locale_fonts(&self.context, self.state.locale);
            log_font_report(&report, self.state.locale);
            self.installed_font_locale = self.state.locale;
            self.font_probe_pending = true;
            self.persist_preferences();
        }
        self.log_morph_preview_timing();
        self.log_sculpt_dab_timing();
        self.process_dialog();
        self.process_export();
        self.process_cancel();
        self.ensure_scan_import_started();
        self.ensure_workspace_load_started();
        self.ensure_worker_started();
        self.ensure_asset_catalog_workers_started();
        self.ensure_skin_asset_worker_started();
        self.ensure_texture_asset_worker_started();

        let root_viewport_commands = output
            .viewport_output
            .get_mut(&ViewportId::ROOT)
            .map(|viewport| std::mem::take(&mut viewport.commands))
            .unwrap_or_default();
        let close_requested =
            self.process_root_viewport_commands(event_loop, root_viewport_commands);

        let repaint_delay = output
            .viewport_output
            .get(&ViewportId::ROOT)
            .map_or(Duration::MAX, |viewport| viewport.repaint_delay);
        if repaint_delay == Duration::ZERO && self.repaint_trace_count < 12 && repaint_trace_asked()
        {
            self.repaint_trace_count += 1;
            log(
                Severity::Debug,
                "repaint_trace",
                &format!("causes={:?}", self.context.repaint_causes()),
            );
        }

        self.egui_state.handle_platform_output_with_event_loop(
            self.window.as_ref(),
            event_loop,
            output.platform_output,
        );

        if close_requested {
            return None;
        }

        let tessellate_started = self.frame_trace.map(|_| Instant::now());
        let clipped_primitives = self
            .context
            .tessellate(output.shapes, output.pixels_per_point);
        let tessellate_ms = elapsed_ms(tessellate_started);
        let paint_started = self.frame_trace.map(|_| Instant::now());
        self.painter.paint_and_update_textures(
            ViewportId::ROOT,
            output.pixels_per_point,
            CLEAR_COLOR,
            &clipped_primitives,
            &output.textures_delta,
            Vec::new(),
            &self.window,
        );
        if let Some(trace) = self.frame_trace.as_mut() {
            trace.record(ui_ms, tessellate_ms, elapsed_ms(paint_started));
        }

        if repaint_delay == Duration::ZERO {
            self.window.request_redraw();
            None
        } else if repaint_delay < Duration::MAX {
            Instant::now().checked_add(repaint_delay)
        } else {
            None
        }
    }

    fn paint_pending_hair_portraits(&mut self) {
        if self.state.pending_hair_portraits.is_empty() {
            return;
        }
        let wanted = std::mem::take(&mut self.state.pending_hair_portraits);
        let Some(render_state) = self.painter.render_state() else {
            self.state.export_hair_style();
            return;
        };
        // Kept between runs, and rebuilt only when the shape or the sample
        // count moves — the same reuse the viewport will want.
        let canvas = match self.portrait_canvas.take() {
            Some(canvas) => canvas.reshaped(
                &render_state.device,
                render_state.target_format,
                crate::hair_portrait::PORTRAIT_SIDE,
            ),
            None => crate::hair_portrait::PortraitTarget::new(
                &render_state.device,
                render_state.target_format,
                crate::hair_portrait::PORTRAIT_SIDE,
            ),
        };
        for target in wanted {
            let only = match target {
                crate::state::HairThumbnailTarget::Part(id) => Some(id),
                crate::state::HairThumbnailTarget::Preset => None,
            };
            match self.paint_hair_portrait(&render_state, &canvas, only) {
                Ok(jpeg) => match target {
                    crate::state::HairThumbnailTarget::Part(id) => {
                        self.state.set_hair_part_thumbnail(id, jpeg);
                    }
                    crate::state::HairThumbnailTarget::Preset => {
                        self.state.set_hair_preset_thumbnail(jpeg);
                    }
                },
                Err(detail) => log(Severity::Warning, "hair_portrait_failed", &detail),
            }
        }
        self.portrait_canvas = Some(canvas);
        self.state.export_hair_style();
    }

    fn paint_hair_portrait(
        &mut self,
        render_state: &egui_wgpu::RenderState,
        canvas: &crate::hair_portrait::PortraitTarget,
        only: Option<u64>,
    ) -> Result<Vec<u8>, String> {
        let camera = self
            .state
            .hair_portrait_camera(only)
            .ok_or_else(|| "there is no head to frame".to_owned())?;
        let scene = self
            .state
            .hair_portrait_scene(&self.context, camera, only)
            .ok_or_else(|| "the layers have nothing to draw".to_owned())?;
        let square = crate::hair_portrait::render_portrait(render_state, canvas, &scene)?;
        crate::thumbnail::encode_square_jpeg(&square, crate::thumbnail::THUMBNAIL_SIDE)
    }

    fn shoot_hair_thumbnail(&mut self) {
        if !self
            .state
            .hair_thumbnail
            .as_ref()
            .is_some_and(|job| job.shoot)
        {
            return;
        }
        let Some(job) = self.state.hair_thumbnail.take() else {
            return;
        };
        let result = self.capture_square(&job);
        if result.is_ok()
            && let Some(square) = job.square
        {
            let part = match &job.target {
                crate::state::HairThumbnailTarget::Part(part_id) => Some(*part_id),
                crate::state::HairThumbnailTarget::Preset => None,
            };
            self.state.hair_shot_flash = Some(crate::state::HairShotFlash {
                square,
                part,
                at: self.context.input(|input| input.time),
            });
        }
        self.state.complete_hair_thumbnail(result);
    }

    #[cfg(target_os = "windows")]
    fn capture_square(&mut self, job: &crate::state::HairThumbnailJob) -> Result<(), String> {
        let square = job
            .square
            .ok_or_else(|| "the viewport never reported the framed square".to_owned())?;
        let hwnd = crate::window_control::windows_hwnd(self.window.as_ref())?;
        let capture =
            crate::thumbnail::capture_screen_square(hwnd, square, self.context.pixels_per_point())?;
        let fresh =
            crate::thumbnail::encode_square_jpeg(&capture, crate::thumbnail::THUMBNAIL_SIDE)?;
        match &job.target {
            crate::state::HairThumbnailTarget::Part(part_id) => {
                self.state.set_hair_part_thumbnail(*part_id, fresh);
                Ok(())
            }
            crate::state::HairThumbnailTarget::Preset => {
                self.state.set_hair_preset_thumbnail(fresh);
                Ok(())
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn capture_square(&mut self, _job: &crate::state::HairThumbnailJob) -> Result<(), String> {
        Err("screen capture is only wired up on Windows".to_owned())
    }

    fn process_root_viewport_commands(
        &mut self,
        event_loop: &ActiveEventLoop,
        commands: Vec<ViewportCommand>,
    ) -> bool {
        let mut close_requested = false;

        let mut maximize_target: Option<bool> = None;
        for command in commands {
            match command {
                ViewportCommand::StartDrag => {
                    self.begin_titlebar_drag();
                }
                ViewportCommand::BeginResize(direction) => {
                    self.begin_window_resize(direction);
                }
                ViewportCommand::Minimized(minimized) => self.window.set_minimized(minimized),
                ViewportCommand::Maximized(maximized) => maximize_target = Some(maximized),
                ViewportCommand::Close => close_requested = true,
                _ => {}
            }
        }
        if let Some(maximized) = maximize_target {
            self.set_window_maximized(maximized);
        }
        if close_requested {
            self.close(event_loop);
        }
        close_requested
    }

    fn begin_titlebar_drag(&self) {
        if let Err(error) = crate::window_control::begin_titlebar_drag(&self.window) {
            log(Severity::Warning, "window_drag_failed", &error);
        }
    }

    fn begin_window_resize(&self, direction: egui::viewport::ResizeDirection) {
        if let Err(error) = crate::window_control::begin_window_resize(&self.window, direction) {
            log(Severity::Warning, "window_resize_failed", &error);
        }
    }

    fn set_window_maximized(&self, maximized: bool) {
        if self.window.is_maximized() != maximized {
            self.window.set_maximized(maximized);
        }
    }

    fn drain_synthetic_pointer_release(&mut self) {
        if !crate::window_control::take_synthetic_pointer_release() {
            return;
        }

        let primary_down = self.context.input(|input| input.pointer.primary_down());
        if !primary_down {
            return;
        }
        let position = self
            .context
            .input(|input| input.pointer.latest_pos())
            .unwrap_or_default();
        self.egui_state
            .egui_input_mut()
            .events
            .push(egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            });

        self.pointer_release_count = self.pointer_release_count.saturating_add(1);
        if should_log_pointer_release(self.pointer_release_count) {
            log(
                Severity::Debug,
                "pointer_release_synthesized",
                &format!(
                    "cleared a pointer press swallowed by a native modal loop (occurrences={})",
                    self.pointer_release_count
                ),
            );
        }
    }

    fn record_new_status(&mut self) {
        let status = &self.state.status;
        let severity = match status.tone {
            crate::state::StatusTone::Error => Severity::Error,
            crate::state::StatusTone::Warning => Severity::Warning,

            crate::state::StatusTone::Neutral
            | crate::state::StatusTone::Info
            | crate::state::StatusTone::Success => Severity::Info,
        };
        let text = crate::i18n::text(self.state.locale, status.key);
        let message = match &status.detail {
            Some(detail) => format!("{text}: {detail}"),
            None => text.to_owned(),
        };

        if self.logged_status.as_deref() == Some(message.as_str()) {
            return;
        }
        self.logged_status = Some(message.clone());
        log(severity, "status", &message);
    }

    fn tick_autosave(&mut self) {
        let Some(store) = self.recovery.as_ref() else {
            return;
        };
        let now = self.session_started.elapsed();

        let edits = self.state.autosave_edit_signal();
        if edits != self.snapshotted_edits {
            self.snapshotted_edits = edits;
            self.autosave.mark_dirty(now);
        }
        if self.state.pending_recovery.is_none() && self.autosave.should_write(now) {
            let snapshot = self.state.recovery_snapshot();
            let result = if snapshot.has_work() {
                store.save(&snapshot)
            } else {
                store.release_snapshot();
                Ok(())
            };
            if let Err(error) = result {
                log(
                    Severity::Warning,
                    "recovery_save_failed",
                    &error.to_string(),
                );
            }
            self.autosave.mark_written(now);
        } else if self.autosave.should_heartbeat(now) {
            if let Err(error) = store.heartbeat() {
                log(
                    Severity::Warning,
                    "recovery_heartbeat_failed",
                    &error.to_string(),
                );
            }
            self.autosave.mark_heartbeat(now);
        }
    }

    fn close(&mut self, event_loop: &ActiveEventLoop) {
        log(
            Severity::Info,
            "close_requested",
            "persisting preferences and exiting",
        );
        self.persist_preferences();

        if let Some(store) = self.recovery.as_ref() {
            store.release();
        }
        let _ = diagnostics::flush();
        if self.state.settings_reset {
            relaunch();
        }
        event_loop.exit();
    }

    fn log_sculpt_dab_timing(&mut self) {
        let Some(timing) = self.state.sculpt_dab_timing else {
            return;
        };
        if timing.serial == self.last_sculpt_timing_serial {
            return;
        }
        self.last_sculpt_timing_serial = timing.serial;
        let sampled = timing.serial <= 3 || timing.serial % 300 == 0 || timing.dab_ms >= 25.0;
        if !sampled || self.sculpt_timing_log_count >= 20 {
            return;
        }
        self.sculpt_timing_log_count += 1;
        log(
            Severity::Debug,
            "sculpt_dab_timing",
            &format!(
                "serial={}; dab_ms={:.3}; changed_vertices={}",
                timing.serial, timing.dab_ms, timing.changed_vertices,
            ),
        );
    }

    fn log_morph_preview_timing(&mut self) {
        let Some(timing) = self.state.morph_preview_timing else {
            return;
        };
        if timing.serial == self.last_morph_timing_serial {
            return;
        }
        self.last_morph_timing_serial = timing.serial;
        let sampled = timing.serial <= 3 || timing.serial % 60 == 0 || timing.total_ms >= 50.0;
        if !sampled || self.morph_timing_log_count >= 20 {
            return;
        }
        self.morph_timing_log_count += 1;
        log(
            Severity::Debug,
            "morph_preview_timing",
            &format!(
                "serial={}; compose_ms={:.3}; eyelid_ms={:.3}; scene_ms={:.3}; total_ms={:.3}",
                timing.serial,
                timing.compose_ms,
                timing.eyelid_ms,
                timing.scene_ms,
                timing.total_ms,
            ),
        );
    }

    fn process_dialog(&mut self) {
        let Some(intent) = self.state.take_dialog_intent() else {
            return;
        };
        let Some(path) = dialogs::show(intent, &self.state) else {
            log(
                Severity::Debug,
                "dialog_cancelled",
                &format!("intent={intent:?}"),
            );
            return;
        };
        log(
            Severity::Info,
            "dialog_selected",
            &format!("intent={intent:?}"),
        );
        match intent {
            crate::state::DialogIntent::OpenScan => self.state.dispatch(Action::LoadScan(path)),
            crate::state::DialogIntent::OpenTextureImage(source_mode) => self
                .state
                .dispatch(Action::AddTextureImage(path, source_mode)),
            crate::state::DialogIntent::ChooseOutput => self
                .state
                .dispatch(Action::SetOutputPath(path.to_string_lossy().into_owned())),
            crate::state::DialogIntent::ChooseVaMRoot => {
                self.state.dispatch(Action::SetVaMRoot(path))
            }
            crate::state::DialogIntent::OpenHeadPreset => {
                self.state.dispatch(Action::OpenHeadPresetFile(path));
            }
        }
        self.persist_preferences();
    }

    fn process_export(&mut self) {
        if self.exports.is_active() {
            return;
        }
        let Some(source_revision) = self.state.take_export_request() else {
            return;
        };
        let snapshot = match export_snapshot_from_state(&self.state) {
            Ok(snapshot) if snapshot.source_revision == source_revision => snapshot,
            Ok(_) => {
                self.state.dispatch(Action::FinishExport {
                    source_revision,
                    outcome: ExportOutcome::Failed(
                        "the fitted result changed before export".to_owned(),
                    ),
                });
                return;
            }
            Err(error) => {
                log(
                    Severity::Error,
                    "export_snapshot_failed",
                    &error.to_string(),
                );
                self.state.dispatch(Action::FinishExport {
                    source_revision,
                    outcome: ExportOutcome::Failed(error.to_string()),
                });
                return;
            }
        };
        log(
            Severity::Info,
            "export_starting",
            &format!(
                "revision={source_revision}; vertices={}; faces={}; output={}",
                snapshot.output.vertices.len(),
                snapshot.output.faces.len(),
                snapshot.output_path.display(),
            ),
        );
        let proxy = self.proxy.clone();
        if let Err(error) = self.exports.start(snapshot, move || {
            let _ = proxy.send_event(RuntimeEvent::WorkerWake);
        }) {
            log(Severity::Error, "export_start_failed", &error);
            self.state.dispatch(Action::FinishExport {
                source_revision,
                outcome: ExportOutcome::Failed(error),
            });
        }
        self.window.request_redraw();
    }

    fn process_cancel(&mut self) {
        if let Some(job_id) = self.state.take_cancel_request() {
            log(
                Severity::Info,
                "job_cancel_requested",
                &format!("job_id={job_id}"),
            );
            self.jobs.cancel(job_id);
        }
    }

    fn log_glyph_probe(&self) {
        let (probe, script) = match self.state.locale {
            Locale::Korean => ('한', "hangul"),
            Locale::Japanese => ('あ', "kana"),
            Locale::ZhHans | Locale::ZhHant => ('汉', "han"),
            Locale::Thai => ('ก', "thai"),
            Locale::Hindi => ('क', "devanagari"),
            Locale::Bengali => ('ক', "bengali"),
            Locale::Russian => ('Я', "cyrillic"),

            Locale::English
            | Locale::Spanish
            | Locale::Portuguese
            | Locale::French
            | Locale::German
            | Locale::Indonesian
            | Locale::Vietnamese => return,
        };
        let sources = theme::glyph_font_sources(&self.context, probe);
        if sources.is_empty() {
            log(
                Severity::Warning,
                "font_glyph_missing",
                &format!(
                    "{script} U+{:04X} resolves to no installed font; labels will show fallback glyphs",
                    probe as u32
                ),
            );
        } else {
            log(
                Severity::Info,
                "font_glyph_resolved",
                &format!("{script} '{probe}' renders with {}", sources.join(", ")),
            );
        }
    }

    fn tick_cache_maintenance(&mut self) {
        if std::mem::take(&mut self.state.cache_clear_requested) {
            let removed = crate::vam_morph_cache::default_cache_root()
                .map(|root| empty_directory(&root))
                .unwrap_or_default();
            log(
                Severity::Info,
                "cache_cleared",
                &format!("{removed} entries removed"),
            );
            self.state.cache_bytes = None;
            self.state.cache_measure_requested = true;
        }
        if std::mem::take(&mut self.state.cache_measure_requested) {
            self.state.cache_bytes = Some(
                crate::vam_morph_cache::default_cache_root()
                    .map(|root| directory_bytes(&root))
                    .unwrap_or_default(),
            );
        }
    }

    fn tick_preferences(&mut self) {
        let now = self.session_started.elapsed();
        if now.saturating_sub(self.preferences_checked) < std::time::Duration::from_secs(2) {
            return;
        }
        self.preferences_checked = now;
        self.persist_preferences();
    }

    fn persist_preferences(&mut self) {
        let Some(store) = self.preferences.as_ref() else {
            return;
        };

        if self.state.settings_reset {
            let defaults = Preferences::default();
            if self.last_saved_preferences.as_ref() == Some(&defaults) {
                return;
            }
            match store.save(&defaults) {
                Ok(()) => self.last_saved_preferences = Some(defaults),
                Err(error) => log(
                    Severity::Warning,
                    "settings_reset_failed",
                    &error.to_string(),
                ),
            }
            return;
        }
        let preferences = preferences_from_state(&self.state);
        if self.last_saved_preferences.as_ref() == Some(&preferences) {
            return;
        }
        if let Err(error) = store.save(&preferences) {
            log(
                Severity::Warning,
                "settings_save_failed",
                &error.to_string(),
            );
            return;
        }
        self.last_saved_preferences = Some(preferences);
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
struct StartupPaths {
    scan: Option<PathBuf>,
    template: Option<PathBuf>,
    result: Option<PathBuf>,
    output: Option<PathBuf>,
    vam: Option<PathBuf>,
}

fn preferences_from_state(state: &AppState) -> Preferences {
    Preferences {
        locale: Some(state.locale),
        morph_name_display: state.morph_name_display,
        shortcuts: state.keymap.to_stored(),
        inspector_width: Some(theme::clamp_inspector_width(state.inspector_width).round() as u16),
        vam_root: state.vam_root.clone(),
        vam_geometry_base_path: state.vam_geometry_base_path.clone(),
        figure_sex: Some(state.figure_sex),
        vam_export_display_name: Some(state.vam_export_display_name.clone()),
        vam_export_group: Some(state.vam_export_group.clone()),
        vam_export_region: Some(state.vam_export_region.clone()),
        vam_export_is_pose_control: Some(state.vam_export_is_pose_control),
        vam_export_bone_correction: Some(state.vam_export_bone_correction),
        custom_head_solid_color_rgb: state.custom_head_solid_color_rgb,
        g2_solid_color_rgb: state.g2_solid_color_rgb,
        wireframe_color_rgb: state.wireframe_color_rgb,
        base_view_mode: state.base_view_mode,
        surface_smooth_passes: state.surface_smooth_passes,
        tooltips_enabled: state.tooltips_enabled,
        hair_toolbox_columns: state.hair_toolbox_columns,
        hair_toolbox_pos: state.hair_toolbox_pos,
        show_one_sided_morphs: state.morph_library.show_one_sided,

        last_skin_id: state
            .selected_skin_id
            .clone()
            .or_else(|| state.last_skin_id.clone()),
        default_skin_id: state.default_skin_id.clone(),
        package_creator: Some(state.var_metadata.creator.clone()),
        package_version: Some(state.var_version_text.clone()),
        package_license: Some(state.var_metadata.license.clone()),
        package_promotional_link: Some(state.var_metadata.promotional_link.clone()),
        viewport_background_mode: state.viewport_background_mode,
        brush_sweep_commit: state.brush_sweep_commit,
        wireframe_visible: state.wireframe_visible,
        wireframe_opacity: state.wireframe_opacity,
        xray_visible: state.xray_visible,
        xray_opacity: state.xray_opacity,
        scan_overlay: state.scan_overlay,
        overlay_opacity: state.overlay_opacity,
        show_result_tear_lacrimals: state.show_result_tear_lacrimals,
        show_result_eyelashes: state.show_result_eyelashes,
        alignment_opacity: state.alignment_opacity,
        alignment_g2_opacity: state.alignment_g2_opacity,
        light_yaw_radians: state.light_yaw_radians,
        lighting_preset: state.lighting_preset,
        light_brightness: state.light_brightness,
        msaa_samples: state.msaa_samples,
        tone_mapping: state.tone_mapping.id(),
        vignette_enabled: state.vignette.enabled,
        vignette_intensity: state.vignette.intensity,
        vignette_smoothness: state.vignette.smoothness,
        vignette_roundness: state.vignette.roundness,
    }
}

/// Settle the sample count against the hardware before the painter exists.
///
/// A throwaway adapter on the same backend the painter will pick, asked one
/// question and dropped. It costs a few milliseconds once, and it is the only
/// place the answer can be had in time: after `Painter::new` the count is
/// already inside egui's pipelines.
fn probe_msaa_samples(wanted: u32) -> u32 {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = wgpu::Backends::DX12;
    descriptor.flags = wgpu::InstanceFlags::empty().with_env();
    let instance = wgpu::Instance::new(descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    }));
    match adapter {
        Ok(adapter) => {
            log(
                Severity::Info,
                "msaa_adapter",
                &format!(
                    "{} - adapter-specific format features {}",
                    adapter.get_info().name,
                    if adapter
                        .features()
                        .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
                    {
                        "available"
                    } else {
                        "absent, so 1x and 4x only"
                    },
                ),
            );
            renderer::resolve_msaa_samples(&adapter, wgpu::TextureFormat::Bgra8UnormSrgb, wanted)
        }
        Err(error) => {
            log(
                Severity::Warning,
                "msaa_probe_failed",
                &format!("running at the default sample count: {error}"),
            );
            renderer::DEFAULT_MSAA_SAMPLES
        }
    }
}

fn apply_saved_preferences(state: &mut AppState, saved: &Preferences) {
    state.dispatch(Action::SetBaseViewMode(saved.base_view_mode));
    state.dispatch(Action::SetSurfaceSmoothPasses(saved.surface_smooth_passes));
    state.dispatch(Action::SetTooltipsEnabled(saved.tooltips_enabled));
    state.dispatch(Action::SetShowOneSidedMorphs(saved.show_one_sided_morphs));
    state.hair_toolbox_columns = saved.hair_toolbox_columns.clamp(1, 2);
    state.hair_toolbox_pos = saved.hair_toolbox_pos;
    state.morph_name_display = saved.morph_name_display;
    state.keymap = crate::shortcuts::Keymap::from_stored(&saved.shortcuts);

    state.dispatch(Action::SetDefaultSkin(saved.default_skin_id.clone()));
    state.dispatch(Action::SetLastSkin(saved.last_skin_id.clone()));
    for (field, value) in [
        (VarMetadataField::Creator, saved.package_creator.clone()),
        (VarMetadataField::Version, saved.package_version.clone()),
        (VarMetadataField::License, saved.package_license.clone()),
        (
            VarMetadataField::PromotionalLink,
            saved.package_promotional_link.clone(),
        ),
    ] {
        if let Some(value) = value {
            state.dispatch(Action::SetVarMetadata(field, value));
        }
    }
    state.dispatch(Action::SetViewportBackgroundMode(
        saved.viewport_background_mode,
    ));
    state.dispatch(Action::SetBrushSweepCommit(saved.brush_sweep_commit));
    state.dispatch(Action::SetCustomHeadSolidColor(
        saved.custom_head_solid_color_rgb,
    ));
    state.dispatch(Action::SetG2SolidColor(saved.g2_solid_color_rgb));
    state.dispatch(Action::SetWireframeColor(saved.wireframe_color_rgb));
    state.dispatch(Action::ToggleWireframe(saved.wireframe_visible));
    state.dispatch(Action::SetWireframeOpacity(saved.wireframe_opacity));
    state.dispatch(Action::ToggleXray(saved.xray_visible));
    state.dispatch(Action::SetXrayOpacity(saved.xray_opacity));
    state.dispatch(Action::ToggleScanOverlay(saved.scan_overlay));
    state.dispatch(Action::SetOverlayOpacity(saved.overlay_opacity));
    state.dispatch(Action::ToggleResultTearLacrimals(
        saved.show_result_tear_lacrimals,
    ));
    state.dispatch(Action::ToggleResultEyelashes(saved.show_result_eyelashes));
    state.dispatch(Action::SetAlignmentOpacity(saved.alignment_opacity));
    state.ensure_visible_alignment_opacity();
    state.dispatch(Action::SetAlignmentG2Opacity(saved.alignment_g2_opacity));
    state.dispatch(Action::SetLightYaw(saved.light_yaw_radians));
    state.dispatch(Action::SetLightingPreset(saved.lighting_preset));
    state.dispatch(Action::SetLightBrightness(saved.light_brightness));
    state.dispatch(Action::SetToneMapping(
        crate::shader_color::ToneMapping::from_id(saved.tone_mapping),
    ));
    state.dispatch(Action::SetMsaaSamples(saved.msaa_samples));
    state.dispatch(Action::SetVignette(crate::post_process::VignetteSettings {
        enabled: saved.vignette_enabled,
        intensity: saved.vignette_intensity,
        smoothness: saved.vignette_smoothness,
        roundness: saved.vignette_roundness,
    }));

    if let Some(value) = saved.vam_export_display_name.clone() {
        state.dispatch(Action::SetVaMExportDisplayName(value));
    }
    if let Some(value) = saved.vam_export_group.clone() {
        state.dispatch(Action::SetVaMExportGroup(value));
    }
    if let Some(value) = saved.vam_export_region.clone() {
        state.dispatch(Action::SetVaMExportRegion(value));
    }
    if let Some(value) = saved.vam_export_is_pose_control {
        state.dispatch(Action::SetVaMExportIsPoseControl(value));
    }
    if let Some(value) = saved.vam_export_bone_correction {
        state.dispatch(Action::SetVaMExportBoneCorrection(value));
    }
    if let Some(width) = saved.inspector_width {
        state.inspector_width = theme::clamp_inspector_width(f32::from(width));
    }

    if let Some(value) = saved.figure_sex {
        state.dispatch(Action::SetFigureSex(value));
    }
}

fn parse_startup_paths(arguments: impl IntoIterator<Item = OsString>) -> StartupPaths {
    let mut paths = StartupPaths::default();
    let mut arguments = arguments.into_iter().skip(1);
    while let Some(argument) = arguments.next() {
        let text = argument.to_string_lossy();
        let target = match text.as_ref() {
            "--scan" => Some(&mut paths.scan),
            "--template" => Some(&mut paths.template),
            "--result" => Some(&mut paths.result),
            "--output" => Some(&mut paths.output),
            "--vam" => Some(&mut paths.vam),
            _ => None,
        };
        if let Some(target) = target {
            if let Some(path) = arguments.next() {
                *target = Some(PathBuf::from(path));
            }
            continue;
        }
        if let Some(value) = text.strip_prefix("--scan=") {
            paths.scan = Some(PathBuf::from(value));
        } else if let Some(value) = text.strip_prefix("--template=") {
            paths.template = Some(PathBuf::from(value));
        } else if let Some(value) = text.strip_prefix("--result=") {
            paths.result = Some(PathBuf::from(value));
        } else if let Some(value) = text.strip_prefix("--output=") {
            paths.output = Some(PathBuf::from(value));
        } else if let Some(value) = text.strip_prefix("--vam=") {
            paths.vam = Some(PathBuf::from(value));
        } else if !text.starts_with('-') && paths.scan.is_none() {
            paths.scan = Some(PathBuf::from(argument));
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_morph_name_display_choice_survives_the_settings_round_trip() {
        let mut state = AppState::default();
        state.morph_name_display = crate::state::MorphNameDisplay::Original;
        let saved = preferences_from_state(&state);

        let mut restarted = AppState::default();
        apply_saved_preferences(&mut restarted, &saved);
        assert_eq!(
            restarted.morph_name_display,
            crate::state::MorphNameDisplay::Original,
            "the persisted choice has to be applied at startup, not merely written"
        );
    }

    #[test]
    fn a_save_before_the_catalog_resolves_keeps_the_remembered_skin() {
        let mut state = AppState::default();
        state.last_skin_id = Some("skin-a".to_owned());
        assert_eq!(
            preferences_from_state(&state).last_skin_id.as_deref(),
            Some("skin-a"),
            "an unresolved selection must not wipe the remembered skin"
        );

        state.selected_skin_id = Some("skin-b".to_owned());
        assert_eq!(
            preferences_from_state(&state).last_skin_id.as_deref(),
            Some("skin-b"),
            "a live selection wins over the remembered one"
        );
    }

    #[test]
    fn synthesized_pointer_release_records_are_throttled_to_one_in_twenty_five() {
        assert!(should_log_pointer_release(1));
        assert!(!should_log_pointer_release(2));
        assert!(!should_log_pointer_release(24));
        assert!(should_log_pointer_release(25));
        assert!(!should_log_pointer_release(26));
        assert!(should_log_pointer_release(50));
        let logged = (1..=100).filter(|&n| should_log_pointer_release(n)).count();
        assert_eq!(logged, 5, "1 then every 25th of 100 occurrences");
    }

    #[test]
    fn job_progress_records_only_stage_changes_and_whole_percent_steps() {
        let stage = JobStage::Fit;

        assert!(should_log_job_progress(None, 7, stage, 0.0));
        let last = Some((7, stage, 0.100));

        assert!(!should_log_job_progress(last, 7, stage, 0.104));
        assert!(!should_log_job_progress(last, 7, stage, 0.1099));

        assert!(should_log_job_progress(last, 7, stage, 0.110));
        assert!(should_log_job_progress(last, 7, JobStage::Validate, 0.100));
        assert!(should_log_job_progress(last, 8, stage, 0.100));

        assert!(should_log_job_progress(
            Some((7, stage, 0.995)),
            7,
            stage,
            1.0
        ));
    }

    #[test]
    fn redraw_event_does_not_schedule_itself_again() {
        assert!(!should_schedule_event_repaint(
            &WindowEvent::RedrawRequested,
            true
        ));
        assert!(!should_schedule_event_repaint(
            &WindowEvent::RedrawRequested,
            false
        ));
        assert!(should_schedule_event_repaint(
            &WindowEvent::Focused(true),
            true
        ));
    }

    #[test]
    fn moving_the_window_schedules_no_frame_while_resizing_still_does() {
        assert!(!should_schedule_event_repaint(
            &WindowEvent::Moved(winit::dpi::PhysicalPosition::new(120, 80)),
            true
        ));
        assert!(should_schedule_event_repaint(
            &WindowEvent::Resized(winit::dpi::PhysicalSize::new(1280, 800)),
            true
        ));
    }

    #[test]
    fn native_window_is_frameless_without_changing_its_identity_or_size_contract() {
        let attributes = native_window_attributes(None);

        assert_eq!(attributes.title, crate::APP_TITLE);
        assert!(
            crate::APP_TITLE.starts_with(crate::APP_NAME),
            "{}",
            crate::APP_TITLE
        );

        assert_eq!(
            WINDOWS_APP_USER_MODEL_ID,
            format!("{}.{}", crate::APP_NAME, crate::APP_NAME),
        );
        assert_eq!(vkit_core::APP_DIR_NAME, crate::APP_NAME);

        assert!(
            crate::APP_TITLE.ends_with(env!("CARGO_PKG_VERSION")),
            "the window says which version it is: {}",
            crate::APP_TITLE
        );
        assert!(!attributes.decorations);
        assert_eq!(
            attributes.inner_size,
            Some(
                LogicalSize::new(boot_window::PREFERRED_WIDTH, boot_window::PREFERRED_HEIGHT)
                    .into()
            )
        );
        assert_eq!(
            attributes.min_inner_size,
            Some(LogicalSize::new(MIN_WIDTH, MIN_HEIGHT).into())
        );

        let cramped =
            Desktop::from_screen(1920.0, 1080.0, 1.5, boot_window::WINDOWS_11_TASKBAR_POINTS);
        let attributes = native_window_attributes(Some(cramped));
        assert_eq!(
            attributes.inner_size,
            Some(LogicalSize::new(1280.0_f64, 672.0_f64).into())
        );
        assert_eq!(
            attributes.min_inner_size,
            Some(LogicalSize::new(MIN_WIDTH, MIN_HEIGHT).into()),
            "the floor does not move with the desktop; it has to clear every one"
        );
    }

    #[test]
    fn camera_shortcuts_use_physical_numpad_keys_and_ignore_text_or_repeats() {
        assert_eq!(
            runtime_shortcut_for_physical_key(
                PhysicalKey::Code(KeyCode::Home),
                ElementState::Pressed,
                false,
                false,
            ),
            Some(RuntimeShortcut::ResetCamera)
        );
        assert_eq!(
            runtime_shortcut_for_physical_key(
                PhysicalKey::Code(KeyCode::NumpadDecimal),
                ElementState::Pressed,
                false,
                false,
            ),
            Some(RuntimeShortcut::ToggleProjection)
        );
        assert_eq!(
            runtime_shortcut_for_physical_key(
                PhysicalKey::Code(KeyCode::Numpad0),
                ElementState::Pressed,
                false,
                false,
            ),
            Some(RuntimeShortcut::ResetCamera)
        );
        assert_eq!(
            runtime_shortcut_for_physical_key(
                PhysicalKey::Code(KeyCode::NumpadDecimal),
                ElementState::Released,
                false,
                false,
            ),
            None
        );

        for (code, view) in [
            (KeyCode::Numpad5, crate::camera::StandardView::Front),
            (KeyCode::Numpad4, crate::camera::StandardView::LeftSide),
            (KeyCode::Numpad6, crate::camera::StandardView::RightSide),
            (KeyCode::Numpad8, crate::camera::StandardView::Top),
            (KeyCode::Numpad2, crate::camera::StandardView::Bottom),
            (
                KeyCode::Numpad7,
                crate::camera::StandardView::FrontUpperLeft,
            ),
            (
                KeyCode::Numpad9,
                crate::camera::StandardView::FrontUpperRight,
            ),
            (
                KeyCode::Numpad1,
                crate::camera::StandardView::FrontLowerLeft,
            ),
            (
                KeyCode::Numpad3,
                crate::camera::StandardView::FrontLowerRight,
            ),
        ] {
            assert_eq!(
                runtime_shortcut_for_physical_key(
                    PhysicalKey::Code(code),
                    ElementState::Pressed,
                    false,
                    false,
                ),
                Some(RuntimeShortcut::StandardView(view))
            );
            assert_eq!(
                runtime_shortcut_for_physical_key(
                    PhysicalKey::Code(code),
                    ElementState::Pressed,
                    false,
                    true,
                ),
                None
            );
        }
        assert_eq!(
            runtime_shortcut_for_physical_key(
                PhysicalKey::Code(KeyCode::Home),
                ElementState::Pressed,
                true,
                false,
            ),
            None
        );
        assert_eq!(
            runtime_shortcut_for_physical_key(
                PhysicalKey::Code(KeyCode::Home),
                ElementState::Pressed,
                false,
                true,
            ),
            None
        );
        assert!(matches!(
            RuntimeShortcut::ResetCamera.into_action(),
            Action::ResetCamera
        ));
        assert!(matches!(
            RuntimeShortcut::ToggleProjection.into_action(),
            Action::ToggleProjection
        ));
    }

    #[test]
    fn startup_surface_progress_is_monotonic_and_matches_the_native_clear_color() {
        let phases = StartupPhase::ALL;
        assert!(
            phases
                .windows(2)
                .all(|pair| pair[0].fraction() < pair[1].fraction())
        );
        assert_eq!(phases.last().unwrap().fraction(), 1.0);

        assert_eq!(
            CLEAR_COLOR,
            [
                f32::from(crate::theme::COLOR_BG.r()) / 255.0,
                f32::from(crate::theme::COLOR_BG.g()) / 255.0,
                f32::from(crate::theme::COLOR_BG.b()) / 255.0,
                1.0,
            ]
        );
        for channel in [
            crate::theme::COLOR_BG,
            crate::theme::COLOR_TEXT,
            crate::theme::COLOR_MUTED,
            crate::theme::COLOR_TRACK,
            crate::theme::COLOR_TRACK_FILL,
        ] {
            let spread = channel
                .r()
                .max(channel.g())
                .max(channel.b())
                .saturating_sub(channel.r().min(channel.g()).min(channel.b()));
            assert!(spread <= 2, "{channel:?} tints the boot screen");
        }

        assert_eq!(
            phases.map(|phase| phase.label(Locale::Korean)),
            [
                "Vkit 준비 중",
                "글꼴 준비 중",
                "그래픽 초기화 중",
                "설정 불러오는 중",
                "G2 불러오는 중",
                "작업공간 준비 중",
                "시작하는 중",
            ]
        );
    }

    #[test]
    fn startup_paths_support_named_and_positional_scan() {
        let named = parse_startup_paths([
            OsString::from("Vkit.exe"),
            OsString::from("--scan"),
            OsString::from("scan.obj"),
            OsString::from("--template=g2.obj"),
            OsString::from("--result"),
            OsString::from("result.obj"),
            OsString::from("--output=generated.obj"),
            OsString::from("--vam=C:\\VaM"),
        ]);
        assert_eq!(named.scan, Some(PathBuf::from("scan.obj")));
        assert_eq!(named.template, Some(PathBuf::from("g2.obj")));
        assert_eq!(named.result, Some(PathBuf::from("result.obj")));
        assert_eq!(named.output, Some(PathBuf::from("generated.obj")));
        assert_eq!(named.vam, Some(PathBuf::from(r"C:\VaM")));

        let positional =
            parse_startup_paths([OsString::from("Vkit.exe"), OsString::from("face.obj")]);
        assert_eq!(positional.scan, Some(PathBuf::from("face.obj")));
    }
}

#[cfg(test)]
mod drop_tests {
    use super::*;

    #[test]
    fn a_dropped_file_is_routed_by_what_it_is() {
        let route = |name: &str| dropped_file_action(std::path::Path::new(name));
        assert!(matches!(route("head.obj"), Some(Action::LoadScan(_))));
        assert!(matches!(route("head.GLB"), Some(Action::LoadScan(_))));
        assert!(matches!(route("head.gltf"), Some(Action::LoadScan(_))));
        assert!(matches!(route("head.fbx"), Some(Action::LoadScan(_))));
        assert!(matches!(route("base.dsf"), Some(Action::LoadTemplate(_))));

        assert!(route("notes.txt").is_none());
        assert!(route("photo.png").is_none());
        assert!(route("no-extension").is_none());
    }

    #[test]
    fn the_numpad_and_the_number_row_are_two_different_keyboards() {
        let numpad = [
            KeyCode::Numpad0,
            KeyCode::Numpad1,
            KeyCode::Numpad2,
            KeyCode::Numpad3,
            KeyCode::Numpad4,
            KeyCode::Numpad5,
            KeyCode::Numpad6,
            KeyCode::Numpad7,
            KeyCode::Numpad8,
            KeyCode::Numpad9,
            KeyCode::NumpadDecimal,
        ];
        for code in numpad {
            assert!(is_numpad(code), "{code:?} is on the numpad");
        }

        for code in [
            KeyCode::Digit0,
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit9,
        ] {
            assert!(!is_numpad(code), "{code:?} belongs to the number row");
        }

        for code in numpad {
            assert!(
                runtime_shortcut_for_physical_key(
                    PhysicalKey::Code(code),
                    ElementState::Pressed,
                    false,
                    false,
                )
                .is_some(),
                "{code:?} is claimed from egui but answers to nothing"
            );
        }
    }

    #[test]
    fn no_key_combination_can_close_the_window_behind_the_user() {
        let context = Context::default();
        assert!(
            !context.options(|options| options.quit_shortcuts.is_empty()),
            "egui is expected to ship a quit shortcut; if this stops being true the guard below \
             is measuring nothing and should be re-derived rather than deleted"
        );

        disarm_quit_shortcuts(&context);

        assert!(
            context.options(|options| options.quit_shortcuts.is_empty()),
            "a quit shortcut survived: Ctrl+Q would close the window with no prompt and no \
             autosave flush, losing whatever was on the head"
        );
    }

    #[test]
    fn catalog_warnings_are_grouped_by_reason_rather_than_counted() {
        let warnings = vec![
            "archive C:/x/A.var: invalid VaM skin preset at C:/x/A.var: ZIP end-of-directory record was not found".to_owned(),
            "archive C:/x/B.var: invalid VaM skin preset at C:/x/B.var: texture is missing".to_owned(),
            "archive C:/x/C.var: invalid VaM skin preset at C:/x/C.var: texture is missing".to_owned(),
            "archive C:/x/D.var: invalid VaM skin preset at C:/x/D.var: texture is missing".to_owned(),
        ];
        let summary = summarise_catalog_warnings(&warnings);

        assert!(summary.starts_with("count=4"), "{summary}");
        assert!(
            summary.contains("3x texture is missing"),
            "the common reason must carry its count: {summary}",
        );
        assert!(
            summary.contains("1x ZIP end-of-directory record was not found"),
            "and the rare one must still be named: {summary}",
        );
        assert!(summary.contains("B.var"), "{summary}");
        assert!(summary.contains("C.var"), "{summary}");
        assert!(!summary.contains("D.var"), "{summary}");
    }
}

#[cfg(debug_assertions)]
fn frame_trace() -> Option<FrameTrace> {
    std::env::var_os("VKIT_TRACE_FRAME").map(|_| FrameTrace::default())
}

#[cfg(not(debug_assertions))]
fn frame_trace() -> Option<FrameTrace> {
    None
}

#[cfg(debug_assertions)]
fn repaint_trace_asked() -> bool {
    std::env::var_os("VKIT_TRACE_REPAINT").is_some()
}

#[cfg(not(debug_assertions))]
fn repaint_trace_asked() -> bool {
    false
}
