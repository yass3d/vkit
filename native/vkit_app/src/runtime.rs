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
    vam_hair::{HairPreviewCoordinator, HairPreviewEvent},
    vam_skin::{SkinPreviewCoordinator, SkinPreviewEvent},
    workflow::{
        ExportCoordinator, ExportWorkerEvent, JobCoordinator, WorkerEvent,
        export_snapshot_from_state, snapshot_from_state,
    },
};

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
        eprintln!("Vkit could not load any Windows UI font; labels may use fallback glyphs");
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
    eprintln!("Vkit UI font chain ({locale:?}): {chain}");
    if !report.korean_ready {
        log(
            Severity::Warning,
            "font_fallback",
            "Korean system font could not be loaded",
        );
        eprintln!("Vkit could not load a Korean system font; labels may use fallback glyphs");
    }
    if !report.locale_ready {
        log(
            Severity::Warning,
            "font_locale_fallback",
            &format!("no primary-script font for {locale:?} could be loaded"),
        );
        eprintln!("Vkit could not load a {locale:?} system font; labels may use fallback glyphs");
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

#[derive(Default)]
struct ScanImportCoordinator {
    receiver: Option<Receiver<ScanImportEvent>>,
}

impl ScanImportCoordinator {
    fn is_active(&self) -> bool {
        self.receiver.is_some()
    }

    fn start(&mut self, path: PathBuf, wake: impl Fn() + Send + 'static) -> Result<(), String> {
        if self.is_active() {
            return Err("a scan import is already running".to_owned());
        }
        let (sender, receiver) = mpsc::channel();
        thread::Builder::new()
            .name("vkit-scan-import".to_owned())
            .spawn(move || {
                let progress_sender = sender.clone();
                let mut last_phase = None;
                let mut last_sent_at = None;
                let outcome = catch_unwind(AssertUnwindSafe(|| {
                    PreparedScan::load_with_progress(&path, |progress| {
                        let now = Instant::now();
                        let phase_changed = last_phase != Some(progress.phase);
                        let interval_elapsed = last_sent_at.is_none_or(|last| {
                            now.duration_since(last) >= SCAN_IMPORT_PROGRESS_INTERVAL
                        });
                        if (phase_changed || progress.progress >= 1.0 || interval_elapsed)
                            && progress_sender
                                .send(ScanImportEvent::Progress {
                                    path: path.clone(),
                                    progress,
                                })
                                .is_ok()
                        {
                            last_phase = Some(progress.phase);
                            last_sent_at = Some(now);
                            wake();
                        }
                    })
                }))
                .map_err(|_| "scan import worker stopped unexpectedly".to_owned())
                .and_then(|result| result.map_err(|error| error.to_string()));
                let _ = sender.send(ScanImportEvent::Finished { path, outcome });
                wake();
            })
            .map_err(|error| error.to_string())?;
        self.receiver = Some(receiver);
        Ok(())
    }

    fn drain(&mut self) -> Vec<ScanImportEvent> {
        let mut events = Vec::new();
        let mut disconnected = false;
        if let Some(receiver) = self.receiver.as_ref() {
            loop {
                match receiver.try_recv() {
                    Ok(event) => events.push(event),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        if disconnected
            || events
                .iter()
                .any(|event| matches!(event, ScanImportEvent::Finished { .. }))
        {
            self.receiver = None;
        }
        events
    }
}

enum WorkspaceLoadEvent {
    Finished {
        path: PathBuf,
        outcome: WorkspaceLoadOutcome,
    },
}

#[derive(Default)]
struct WorkspaceLoadCoordinator {
    receiver: Option<Receiver<WorkspaceLoadEvent>>,
}

impl WorkspaceLoadCoordinator {
    fn is_active(&self) -> bool {
        self.receiver.is_some()
    }

    fn start(
        &mut self,
        job: WorkspaceLoadJob,
        wake: impl Fn() + Send + 'static,
    ) -> Result<(), String> {
        if self.is_active() {
            return Err("a workspace load is already running".to_owned());
        }
        let (sender, receiver) = mpsc::channel();
        thread::Builder::new()
            .name("vkit-workspace-load".to_owned())
            .spawn(move || {
                let path = job.path().to_path_buf();
                let kind = job.kind();
                let outcome = catch_unwind(AssertUnwindSafe(|| job.run())).unwrap_or_else(|_| {
                    WorkspaceLoadOutcome::failure(
                        kind,
                        "workspace load worker stopped unexpectedly".to_owned(),
                    )
                });
                let _ = sender.send(WorkspaceLoadEvent::Finished { path, outcome });
                wake();
            })
            .map_err(|error| error.to_string())?;
        self.receiver = Some(receiver);
        Ok(())
    }

    fn drain(&mut self) -> Vec<WorkspaceLoadEvent> {
        let mut events = Vec::new();
        let mut disconnected = false;
        if let Some(receiver) = self.receiver.as_ref() {
            loop {
                match receiver.try_recv() {
                    Ok(event) => events.push(event),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        if disconnected
            || events
                .iter()
                .any(|event| matches!(event, WorkspaceLoadEvent::Finished { .. }))
        {
            self.receiver = None;
        }
        events
    }
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
        "obj" | "glb" | "fbx" => Some(Action::LoadScan(path.to_path_buf())),
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
        PhysicalKey::Code(KeyCode::Home | KeyCode::NumpadDecimal) => {
            Some(RuntimeShortcut::ResetCamera)
        }
        PhysicalKey::Code(KeyCode::Numpad5) => Some(RuntimeShortcut::ToggleProjection),

        PhysicalKey::Code(KeyCode::Numpad1) => Some(RuntimeShortcut::StandardView(
            crate::camera::StandardView::Back,
        )),
        PhysicalKey::Code(KeyCode::Numpad3) => Some(RuntimeShortcut::StandardView(
            crate::camera::StandardView::RightSide,
        )),
        PhysicalKey::Code(KeyCode::Numpad7) => Some(RuntimeShortcut::StandardView(
            crate::camera::StandardView::Top,
        )),
        PhysicalKey::Code(KeyCode::Numpad9) => Some(RuntimeShortcut::StandardView(
            crate::camera::StandardView::Bottom,
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
    hair_assets: HairPreviewCoordinator,
    proxy: EventLoopProxy<RuntimeEvent>,
    preferences: Option<PreferenceStore>,

    installed_font_locale: Locale,

    font_probe_pending: bool,
    last_morph_timing_serial: u64,
    morph_timing_log_count: u8,
    last_sculpt_timing_serial: u64,
    sculpt_timing_log_count: u8,
    repaint_trace_count: u8,

    frame_trace: Option<FrameTrace>,

    pointer_release_count: u32,

    last_logged_job_progress: Option<(u64, JobStage, f32)>,

    recovery: Option<crate::recovery::RecoveryStore>,
    autosave: crate::recovery::AutosaveSchedule,

    session_started: Instant,

    snapshotted_revision: u64,

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
                    eprintln!("Vkit settings load failed: {error}");
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

        let recovery = crate::recovery::RecoveryStore::discover();
        let abandoned = recovery.as_ref().and_then(|store| {
            matches!(
                store.inspect(std::time::SystemTime::now()),
                crate::recovery::LockState::Stale
            )
            .then(|| store.load())
            .flatten()
        });
        if let Some(store) = recovery.as_ref()
            && let Err(error) = store.claim()
        {
            log(
                Severity::Warning,
                "recovery_lock_failed",
                &format!("autosave is off for this session: {error}"),
            );
        }
        let font_report = theme::configure_context(&context, startup_locale);
        log_font_report(&font_report, startup_locale);
        if let Some(surface) = startup_surface.as_ref() {
            surface.paint(StartupPhase::Font);
            surface.paint(StartupPhase::Gpu);
        }

        let mut setup = WgpuSetupCreateNew::from_display_handle(event_loop.owned_display_handle());
        setup.instance_descriptor.backends = wgpu::Backends::DX12;
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
                required_features: wgpu::Features::empty(),
                required_limits: limits,
                memory_hints: wgpu::MemoryHints::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                trace: wgpu::Trace::Off,
            }
        });

        let configuration = WgpuConfiguration {
            wgpu_setup: setup.into(),

            surface: egui_wgpu::SurfaceConfig::LOW_LATENCY,
            ..Default::default()
        };
        let renderer_options = RendererOptions {
            msaa_samples: renderer::MSAA_SAMPLES,
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
            eprintln!("Vkit mesh renderer unavailable: {error}");
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
        state.dispatch(Action::SetBaseViewMode(saved.base_view_mode));
        state.dispatch(Action::SetSurfaceSmoothPasses(saved.surface_smooth_passes));
        state.dispatch(Action::SetTooltipsEnabled(saved.tooltips_enabled));

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
        state.dispatch(Action::SetAmbientOcclusion(
            crate::ambient_occlusion::AmbientOcclusionSettings {
                enabled: saved.occlusion_enabled,
                intensity: saved.occlusion_intensity,
                radius: saved.occlusion_radius,
            },
        ));
        state.dispatch(Action::SetBloom(crate::post_process::BloomSettings {
            enabled: saved.bloom_enabled,
            intensity: saved.bloom_intensity,
            threshold: saved.bloom_threshold,
            soft_knee: saved.bloom_soft_knee,
            radius: saved.bloom_radius,
        }));
        state.dispatch(Action::SetVignette(crate::post_process::VignetteSettings {
            enabled: saved.vignette_enabled,
            intensity: saved.vignette_intensity,
            smoothness: saved.vignette_smoothness,
            roundness: saved.vignette_roundness,
        }));

        if let Some(value) = saved.vam_export_display_name {
            state.dispatch(Action::SetVaMExportDisplayName(value));
        }
        if let Some(value) = saved.vam_export_group {
            state.dispatch(Action::SetVaMExportGroup(value));
        }
        if let Some(value) = saved.vam_export_region {
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
        Ok(Self {
            logged_status: None,
            window,
            context,
            egui_state,
            painter,
            state,
            scan_imports: ScanImportCoordinator::default(),
            workspace_loads: WorkspaceLoadCoordinator::default(),
            jobs: JobCoordinator::default(),
            exports: ExportCoordinator::default(),
            vam_appearance_assets: VaMCoordinator::default(),
            builtin_morph_assets: VaMCoordinator::default(),
            skin_assets: SkinPreviewCoordinator::default(),
            texture_assets: TextureProjectCoordinator::default(),
            hair_assets: HairPreviewCoordinator::default(),
            proxy,
            preferences,
            installed_font_locale: startup_locale,
            font_probe_pending: true,
            last_morph_timing_serial: 0,
            morph_timing_log_count: 0,
            last_sculpt_timing_serial: 0,
            sculpt_timing_log_count: 0,
            repaint_trace_count: 0,
            frame_trace: std::env::var_os("VKIT_TRACE_FRAME").map(|_| FrameTrace::default()),
            pointer_release_count: 0,
            last_logged_job_progress: None,
            recovery,
            autosave: crate::recovery::AutosaveSchedule::default(),
            session_started: Instant::now(),
            snapshotted_revision: 0,
            last_saved_preferences: None,
            preferences_checked: std::time::Duration::ZERO,
        })
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
        self.poll_hair_assets();
    }

    fn render(&mut self, event_loop: &ActiveEventLoop) -> Option<Instant> {
        self.pump_lanes();

        self.drain_synthetic_pointer_release();

        if self.state.hair_settle_seconds > 0.0 {
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
        self.ensure_hair_asset_worker_started();

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
        if repaint_delay == Duration::ZERO
            && self.repaint_trace_count < 12
            && std::env::var_os("VKIT_TRACE_REPAINT").is_some()
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

        if self.state.revision != self.snapshotted_revision {
            self.snapshotted_revision = self.state.revision;
            self.autosave.mark_dirty(now);
        }
        if self.autosave.should_write(now) {
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

    fn ensure_scan_import_started(&mut self) {
        if self.scan_imports.is_active() {
            return;
        }
        let Some(path) = self.state.take_scan_import_request() else {
            return;
        };
        let failure_path = path.clone();
        let proxy = self.proxy.clone();
        if let Err(error) = self.scan_imports.start(path, move || {
            let _ = proxy.send_event(RuntimeEvent::WorkerWake);
        }) {
            self.state.dispatch(Action::FinishScanImport {
                path: failure_path,
                outcome: Err(error),
            });
        }
    }

    fn poll_scan_import(&mut self) {
        for event in self.scan_imports.drain() {
            match event {
                ScanImportEvent::Progress { path, progress } => {
                    self.state.dispatch(Action::ReportImportProgress {
                        path,
                        progress: crate::state::ImportProgress::from_mesh_import(progress),
                    });
                }
                ScanImportEvent::Finished { path, outcome } => {
                    if let Err(error) = &outcome {
                        log(Severity::Error, "scan_import_failed", error);
                    }
                    self.state
                        .dispatch(Action::FinishScanImport { path, outcome });
                    self.persist_preferences();
                }
            }
        }
    }

    fn ensure_workspace_load_started(&mut self) {
        if self.workspace_loads.is_active() {
            return;
        }
        let Some(job) = self.state.take_workspace_load_request() else {
            return;
        };
        let path = job.path().to_path_buf();
        let kind = job.kind();
        log(
            Severity::Info,
            "workspace_load_starting",
            &format!("kind={kind:?}; path={}", path.display()),
        );
        let proxy = self.proxy.clone();
        if let Err(error) = self.workspace_loads.start(job, move || {
            let _ = proxy.send_event(RuntimeEvent::WorkerWake);
        }) {
            log(Severity::Error, "workspace_load_start_failed", &error);
            self.state.dispatch(Action::FinishWorkspaceLoad {
                path,
                outcome: WorkspaceLoadOutcome::failure(kind, error),
            });
        }
    }

    fn poll_workspace_load(&mut self) {
        for event in self.workspace_loads.drain() {
            match event {
                WorkspaceLoadEvent::Finished { path, outcome } => {
                    self.state
                        .dispatch(Action::FinishWorkspaceLoad { path, outcome });
                    self.persist_preferences();
                }
            }
        }
    }

    fn ensure_worker_started(&mut self) {
        if self.jobs.is_active() || self.state.active_job().is_none() {
            return;
        }
        let (job_id, source_revision) = self.state.active_job().expect("checked active job");
        let snapshot_started = Instant::now();
        let snapshot = match snapshot_from_state(&self.state) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                log(Severity::Error, "job_snapshot_failed", &error.to_string());
                self.state.dispatch(Action::FinishGeneration {
                    job_id,
                    source_revision,
                    outcome: GenerationOutcome::Failed(error.to_string()),
                    morph_available: false,
                });
                return;
            }
        };
        let snapshot_elapsed = snapshot_started.elapsed();
        let proxy = self.proxy.clone();
        log(
            Severity::Info,
            "job_starting",
            &format!(
                "job_id={job_id}; revision={source_revision}; snapshot_ms={:.3}; pins={}; scan_vertices={}; template_vertices={}; scale_xyz=[{:.8},{:.8},{:.8}]; translation_cm=[{:.6},{:.6},{:.6}]; rotation_degrees=[{:.4},{:.4},{:.4}]",
                snapshot_elapsed.as_secs_f64() * 1_000.0,
                snapshot.pins.len(),
                snapshot.scan.vertices.len(),
                snapshot.template_base.vertices.len(),
                self.state.transform.scale_xyz[0],
                self.state.transform.scale_xyz[1],
                self.state.transform.scale_xyz[2],
                self.state.transform.translation_cm[0],
                self.state.transform.translation_cm[1],
                self.state.transform.translation_cm[2],
                self.state.transform.rotation_degrees[0],
                self.state.transform.rotation_degrees[1],
                self.state.transform.rotation_degrees[2],
            ),
        );
        if let Err(error) = self.jobs.start(snapshot, move || {
            let _ = proxy.send_event(RuntimeEvent::WorkerWake);
        }) {
            log(Severity::Error, "job_start_failed", &error);
            self.state.dispatch(Action::FinishGeneration {
                job_id,
                source_revision,
                outcome: GenerationOutcome::Failed(error),
                morph_available: false,
            });
            return;
        }
        self.persist_preferences();
    }

    fn poll_worker(&mut self) {
        for event in self.jobs.drain() {
            match event {
                WorkerEvent::Progress {
                    job_id,
                    source_revision,
                    progress,
                } => {
                    if should_log_job_progress(
                        self.last_logged_job_progress,
                        job_id,
                        progress.stage,
                        progress.fraction,
                    ) {
                        self.last_logged_job_progress =
                            Some((job_id, progress.stage, progress.fraction));
                        log(
                            Severity::Debug,
                            "job_progress",
                            &format!(
                                "job_id={job_id}; stage={:?}; fraction={:.3}",
                                progress.stage, progress.fraction
                            ),
                        );
                    }
                    self.state.dispatch(Action::ReportProgress {
                        job_id,
                        source_revision,
                        progress,
                    });
                }
                WorkerEvent::Finished {
                    job_id,
                    source_revision,
                    outcome,
                } => {
                    match &outcome {
                        GenerationOutcome::Success {
                            output,
                            fit_reference_value,
                        } => log(
                            Severity::Info,
                            "job_completed",
                            &format!(
                                "job_id={job_id}; vertices={}; faces={}; eye_reference={fit_reference_value}",
                                output.vertices.len(),
                                output.faces.len()
                            ),
                        ),
                        GenerationOutcome::Failed(error) => {
                            log(Severity::Error, "job_failed", error)
                        }
                        GenerationOutcome::Cancelled => {
                            log(Severity::Info, "job_cancelled", &format!("job_id={job_id}"))
                        }
                    }
                    let morph_available = self.state.workspace.eye_morph.is_some();
                    self.state.dispatch(Action::FinishGeneration {
                        job_id,
                        source_revision,
                        outcome,
                        morph_available,
                    });
                }
            }
        }
    }

    fn poll_export(&mut self) {
        for event in self.exports.drain() {
            match event {
                ExportWorkerEvent::Progress {
                    source_revision,
                    fraction,
                } => {
                    self.state.dispatch(Action::ReportExportProgress {
                        source_revision,
                        fraction,
                    });
                }
                ExportWorkerEvent::Finished {
                    source_revision,
                    outcome,
                } => {
                    match &outcome {
                        ExportOutcome::Success { receipt } => log(
                            Severity::Info,
                            "export_completed",
                            &format!(
                                "committed_paths={}",
                                receipt
                                    .committed_paths
                                    .iter()
                                    .map(|path| path.display().to_string())
                                    .collect::<Vec<_>>()
                                    .join(" | ")
                            ),
                        ),
                        ExportOutcome::Failed(error) => {
                            log(Severity::Error, "export_failed", error)
                        }
                    }
                    self.state.dispatch(Action::FinishExport {
                        source_revision,
                        outcome,
                    });
                    self.persist_preferences();
                }
            }
        }
    }

    fn ensure_asset_catalog_workers_started(&mut self) {
        if !self.vam_appearance_assets.is_active()
            && let Some(request) = self.state.take_vam_appearance_work()
        {
            let fallback = request.clone();
            let proxy = self.proxy.clone();
            if let Err(error) = self.vam_appearance_assets.start(request, move || {
                let _ = proxy.send_event(RuntimeEvent::WorkerWake);
            }) {
                log(
                    Severity::Error,
                    "vam_appearance_worker_start_failed",
                    &error,
                );
                self.apply_asset_catalog_event(fallback.failure_event(error));
            }
        }
        if !self.builtin_morph_assets.is_active()
            && let Some(request) = self.state.take_vam_morph_work()
        {
            let fallback = request.clone();
            let proxy = self.proxy.clone();
            if let Err(error) = self.builtin_morph_assets.start(request, move || {
                let _ = proxy.send_event(RuntimeEvent::WorkerWake);
            }) {
                log(Severity::Error, "builtin_morph_worker_start_failed", &error);
                self.apply_asset_catalog_event(fallback.failure_event(error));
            }
        }
    }

    fn poll_vam_assets(&mut self) {
        let appearance_events = self.vam_appearance_assets.drain();
        for event in appearance_events {
            self.apply_asset_catalog_event(event);
        }
        let morph_events = self.builtin_morph_assets.drain();
        for event in morph_events {
            self.apply_asset_catalog_event(event);
        }
    }

    fn ensure_skin_asset_worker_started(&mut self) {
        if self.skin_assets.is_active() {
            return;
        }
        let Some(request) = self.state.take_skin_work() else {
            return;
        };
        let request_id = request.request_id;
        let preset_id = request.preset.stable_id.clone();
        let proxy = self.proxy.clone();
        if let Err(error) = self.skin_assets.start(request, move || {
            let _ = proxy.send_event(RuntimeEvent::WorkerWake);
        }) {
            log(Severity::Error, "skin_worker_start_failed", &error);
            self.state.dispatch(Action::FinishVaMSkin {
                request_id,
                preset_id,
                outcome: Err(error),
            });
        }
    }

    fn poll_skin_assets(&mut self) {
        for SkinPreviewEvent {
            request_id,
            preset_id,
            outcome,
        } in self.skin_assets.drain()
        {
            match &outcome {
                Ok(preview) => log(
                    Severity::Debug,
                    "skin_preview_ready",
                    &format!(
                        "id={preset_id}; face={}x{}; torso={}x{}; triangles={}",
                        preview.face.width,
                        preview.face.height,
                        preview.torso.width,
                        preview.torso.height,
                        preview.geometry.triangles.len(),
                    ),
                ),
                Err(error) => log(Severity::Error, "skin_preview_failed", error),
            }
            self.state.dispatch(Action::FinishVaMSkin {
                request_id,
                preset_id,
                outcome,
            });
        }
    }

    fn ensure_texture_asset_worker_started(&mut self) {
        if self.texture_assets.is_active() {
            return;
        }
        let Some(request) = self.state.take_texture_work() else {
            return;
        };
        let request_id = request.request_id();
        let fallback = request.clone();
        let proxy = self.proxy.clone();
        if let Err(error) = self.texture_assets.start(request, move || {
            let _ = proxy.send_event(RuntimeEvent::WorkerWake);
        }) {
            log(Severity::Error, "texture_worker_start_failed", &error);
            match fallback {
                crate::texture_project::TextureWorkRequest::Decode(request) => {
                    self.state.dispatch(Action::FinishTextureDecode {
                        request_id,
                        layer_id: request.layer_id,
                        outcome: Err(error),
                    });
                }
                crate::texture_project::TextureWorkRequest::Bake(_) => {
                    self.state.dispatch(Action::FinishTextureBake {
                        request_id,
                        outcome: Err(error),
                    });
                }
            }
        }
    }

    fn poll_texture_assets(&mut self) {
        for event in self.texture_assets.drain() {
            match event {
                TextureWorkerEvent::DecodeFinished {
                    request_id,
                    layer_id,
                    outcome,
                } => {
                    match &outcome {
                        Ok(image) => log(
                            Severity::Info,
                            "texture_layer_ready",
                            &format!("layer={layer_id}; image={}x{}", image.width, image.height),
                        ),
                        Err(error) => log(Severity::Error, "texture_layer_failed", error),
                    }
                    self.state.dispatch(Action::FinishTextureDecode {
                        request_id,
                        layer_id,
                        outcome,
                    });
                }
                TextureWorkerEvent::BakeFinished {
                    request_id,
                    outcome,
                } => {
                    match &outcome {
                        Ok(baked) => log(
                            Severity::Debug,
                            "texture_bake_ready",
                            &format!(
                                "channels={}; request_id={}",
                                baked.images.len(),
                                baked.request_id
                            ),
                        ),
                        Err(error) => log(Severity::Error, "texture_bake_failed", error),
                    }
                    self.state.dispatch(Action::FinishTextureBake {
                        request_id,
                        outcome,
                    });
                }
            }
        }
    }

    fn ensure_hair_asset_worker_started(&mut self) {
        if self.hair_assets.is_active() {
            return;
        }
        let Some(request) = self.state.take_hair_work() else {
            return;
        };
        let request_id = request.request_id;
        let preset_id = request.preset.stable_id.clone();
        let proxy = self.proxy.clone();
        if let Err(error) = self.hair_assets.start(request, move || {
            let _ = proxy.send_event(RuntimeEvent::WorkerWake);
        }) {
            log(Severity::Error, "hair_worker_start_failed", &error);
            self.state.dispatch(Action::FinishVaMHair {
                request_id,
                preset_id,
                outcome: Err(error),
            });
        }
    }

    fn poll_hair_assets(&mut self) {
        for HairPreviewEvent {
            request_id,
            preset_id,
            outcome,
        } in self.hair_assets.drain()
        {
            match &outcome {
                Ok(preview) => log(
                    Severity::Info,
                    "hair_preview_ready",
                    &format!(
                        "id={preset_id}; parts={}; scalps={}; skipped={}; strands={}",
                        preview.parts.len(),
                        preview.scalps.len(),
                        preview.skipped_parts.len(),
                        preview
                            .parts
                            .iter()
                            .map(|part| part.strands.len())
                            .sum::<usize>()
                    ),
                ),
                Err(error) => log(Severity::Error, "hair_preview_failed", error),
            }
            self.state.dispatch(Action::FinishVaMHair {
                request_id,
                preset_id,
                outcome,
            });
        }
    }

    fn apply_asset_catalog_event(&mut self, event: VaMWorkerEvent) {
        match event {
            VaMWorkerEvent::AppearanceProgress {
                catalog_revision,
                fraction,
            } => self.state.dispatch(Action::ReportVaMCatalogProgress {
                catalog_revision,
                fraction,
            }),
            VaMWorkerEvent::Appearance {
                catalog_revision,
                outcome,
            } => {
                match &outcome {
                    Ok(payload) => {
                        log(
                            Severity::Info,
                            "vam_appearance_ready",
                            &format!(
                                "revision={catalog_revision}; skins={}",
                                payload.skin_presets.len()
                            ),
                        );
                        if let Some(first) = payload.appearance_warnings.first() {
                            log(
                                Severity::Warning,
                                "vam_catalog_appearance_warnings",
                                &format!(
                                    "count={}; first={first}",
                                    payload.appearance_warnings.len()
                                ),
                            );
                        }
                    }
                    Err(error) => log(Severity::Error, "vam_catalog_failed", error),
                }
                self.state.dispatch(Action::FinishVaMCatalog {
                    catalog_revision,
                    outcome,
                });
                self.persist_preferences();
            }
            VaMWorkerEvent::MorphCatalog {
                catalog_revision,
                outcome,
            } => {
                match &outcome {
                    Ok(payload) => {
                        let receipt = &payload.morph_cache_receipt;
                        log(
                            Severity::Info,
                            "builtin_morph_catalog_ready",
                            &format!(
                                "revision={catalog_revision}; disposition={:?}; morphs={}; skipped={}; source_bank_bytes={}; cache={}",
                                receipt.disposition,
                                receipt.morph_count,
                                receipt.skipped_count,
                                receipt.source_bytes,
                                receipt.path.display()
                            ),
                        );
                        if let Some(first) = payload.morph_cache_warnings.first() {
                            log(
                                Severity::Warning,
                                "builtin_morph_catalog_warnings",
                                &format!(
                                    "count={}; first={first}",
                                    payload.morph_cache_warnings.len()
                                ),
                            );
                        }
                    }
                    Err(error) => {
                        log(
                            Severity::Warning,
                            "builtin_morph_catalog_unavailable",
                            error,
                        );
                    }
                }
                self.state.dispatch(Action::FinishVaMMorphCatalog {
                    catalog_revision,
                    outcome,
                });
            }
            VaMWorkerEvent::Morph {
                catalog_revision,
                geometry_revision,
                control_id,
                outcome,
            } => {
                match &outcome {
                    Ok(payload) => log(
                        Severity::Debug,
                        "builtin_morph_ready",
                        &format!(
                            "id={control_id}; active_vertices={}; unsupported_formulas={}",
                            payload.target.compatibility.active_vertex_count,
                            payload.unsupported_formula_count
                        ),
                    ),
                    Err(error) => log(Severity::Error, "builtin_morph_failed", error),
                }
                self.state.dispatch(Action::FinishVaMMorph {
                    catalog_revision,
                    geometry_revision,
                    control_id,
                    outcome,
                });
            }
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
        let preferences = Preferences {
            locale: Some(self.state.locale),
            morph_name_display: self.state.morph_name_display,
            inspector_width: Some(
                theme::clamp_inspector_width(self.state.inspector_width).round() as u16,
            ),
            vam_root: self.state.vam_root.clone(),
            vam_geometry_base_path: self.state.vam_geometry_base_path.clone(),
            figure_sex: Some(self.state.figure_sex),
            vam_export_display_name: Some(self.state.vam_export_display_name.clone()),
            vam_export_group: Some(self.state.vam_export_group.clone()),
            vam_export_region: Some(self.state.vam_export_region.clone()),
            vam_export_is_pose_control: Some(self.state.vam_export_is_pose_control),
            vam_export_bone_correction: Some(self.state.vam_export_bone_correction),
            custom_head_solid_color_rgb: self.state.custom_head_solid_color_rgb,
            g2_solid_color_rgb: self.state.g2_solid_color_rgb,
            wireframe_color_rgb: self.state.wireframe_color_rgb,
            base_view_mode: self.state.base_view_mode,
            surface_smooth_passes: self.state.surface_smooth_passes,
            tooltips_enabled: self.state.tooltips_enabled,

            last_skin_id: self.state.selected_skin_id.clone(),
            default_skin_id: self.state.default_skin_id.clone(),
            package_creator: Some(self.state.var_metadata.creator.clone()),
            package_version: Some(self.state.var_version_text.clone()),
            package_license: Some(self.state.var_metadata.license.clone()),
            package_promotional_link: Some(self.state.var_metadata.promotional_link.clone()),
            viewport_background_mode: self.state.viewport_background_mode,
            wireframe_visible: self.state.wireframe_visible,
            wireframe_opacity: self.state.wireframe_opacity,
            xray_visible: self.state.xray_visible,
            xray_opacity: self.state.xray_opacity,
            scan_overlay: self.state.scan_overlay,
            overlay_opacity: self.state.overlay_opacity,
            show_result_tear_lacrimals: self.state.show_result_tear_lacrimals,
            show_result_eyelashes: self.state.show_result_eyelashes,
            alignment_opacity: self.state.alignment_opacity,
            alignment_g2_opacity: self.state.alignment_g2_opacity,
            light_yaw_radians: self.state.light_yaw_radians,
            lighting_preset: self.state.lighting_preset,
            light_brightness: self.state.light_brightness,
            tone_mapping: self.state.tone_mapping.id(),
            vignette_enabled: self.state.vignette.enabled,
            vignette_intensity: self.state.vignette.intensity,
            vignette_smoothness: self.state.vignette.smoothness,
            vignette_roundness: self.state.vignette.roundness,
            bloom_enabled: self.state.bloom.enabled,
            bloom_intensity: self.state.bloom.intensity,
            bloom_threshold: self.state.bloom.threshold,
            bloom_soft_knee: self.state.bloom.soft_knee,
            bloom_radius: self.state.bloom.radius,
            occlusion_enabled: self.state.ambient_occlusion.enabled,
            occlusion_intensity: self.state.ambient_occlusion.intensity,
            occlusion_radius: self.state.ambient_occlusion.radius,
        };
        if self.last_saved_preferences.as_ref() == Some(&preferences) {
            return;
        }
        if let Err(error) = store.save(&preferences) {
            log(
                Severity::Warning,
                "settings_save_failed",
                &error.to_string(),
            );
            eprintln!("Vkit settings save failed: {error}");
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
    fn scan_import_progress_keeps_the_receiver_alive_until_the_terminal_event() {
        let (sender, receiver) = mpsc::channel();
        let mut coordinator = ScanImportCoordinator {
            receiver: Some(receiver),
        };
        let path = PathBuf::from("head.obj");
        sender
            .send(ScanImportEvent::Progress {
                path: path.clone(),
                progress: MeshImportProgress {
                    phase: crate::importers::MeshImportPhase::MeshLoading,
                    progress: 0.25,
                    source_triangles: None,
                },
            })
            .unwrap();

        let first = coordinator.drain();
        assert!(matches!(
            first.as_slice(),
            [ScanImportEvent::Progress { path: event_path, .. }] if event_path == &path
        ));
        assert!(coordinator.is_active());

        sender
            .send(ScanImportEvent::Progress {
                path: path.clone(),
                progress: MeshImportProgress {
                    phase: crate::importers::MeshImportPhase::Simplification,
                    progress: 0.75,
                    source_triangles: None,
                },
            })
            .unwrap();
        sender
            .send(ScanImportEvent::Finished {
                path: path.clone(),
                outcome: Err("expected test terminal event".to_owned()),
            })
            .unwrap();

        let terminal = coordinator.drain();
        assert!(matches!(
            terminal.as_slice(),
            [
                ScanImportEvent::Progress { path: progress_path, .. },
                ScanImportEvent::Finished { path: finished_path, .. },
            ] if progress_path == &path && finished_path == &path
        ));
        assert!(!coordinator.is_active());
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
            Some(RuntimeShortcut::ResetCamera)
        );
        assert_eq!(
            runtime_shortcut_for_physical_key(
                PhysicalKey::Code(KeyCode::Numpad5),
                ElementState::Pressed,
                false,
                false,
            ),
            Some(RuntimeShortcut::ToggleProjection)
        );
        assert_eq!(
            runtime_shortcut_for_physical_key(
                PhysicalKey::Code(KeyCode::Numpad5),
                ElementState::Released,
                false,
                false,
            ),
            None
        );

        for (code, view) in [
            (KeyCode::Numpad1, crate::camera::StandardView::Back),
            (KeyCode::Numpad3, crate::camera::StandardView::RightSide),
            (KeyCode::Numpad7, crate::camera::StandardView::Top),
            (KeyCode::Numpad9, crate::camera::StandardView::Bottom),
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
        assert!(matches!(route("head.fbx"), Some(Action::LoadScan(_))));
        assert!(matches!(route("base.dsf"), Some(Action::LoadTemplate(_))));

        assert!(route("notes.txt").is_none());
        assert!(route("photo.png").is_none());
        assert!(route("no-extension").is_none());
    }
}
