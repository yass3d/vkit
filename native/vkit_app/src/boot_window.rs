use std::path::Path;

use crate::{
    diagnostics::{self, Severity},
    i18n::{Locale, TextKey, text},
    persistence::PreferenceStore,
};

pub const PREFERRED_WIDTH: f64 = 1_600.0;
pub const PREFERRED_HEIGHT: f64 = 920.0;

pub const MIN_WIDTH: f64 = 1_040.0;
pub const MIN_HEIGHT: f64 = 560.0;

#[cfg(test)]
pub const WINDOWS_11_TASKBAR_POINTS: f64 = 48.0;

#[cfg(test)]
pub const WINDOWS_10_TASKBAR_POINTS: f64 = 40.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Desktop {
    pub work_width: f64,

    pub work_height: f64,

    pub scale: f64,
}

impl Desktop {
    #[cfg(test)]
    #[must_use]
    pub fn from_screen(width: f64, height: f64, scale: f64, taskbar_points: f64) -> Self {
        Self {
            work_width: width,
            work_height: (height - taskbar_points * scale).max(0.0),
            scale,
        }
    }

    #[must_use]
    pub fn usable_points(self) -> (f64, f64) {
        let scale = if self.scale.is_finite() && self.scale > 0.0 {
            self.scale
        } else {
            1.0
        };
        (
            (self.work_width / scale).max(0.0),
            (self.work_height / scale).max(0.0),
        )
    }

    #[must_use]
    pub fn fits_minimum_window(self) -> bool {
        let (width, height) = self.usable_points();
        width >= MIN_WIDTH && height >= MIN_HEIGHT
    }

    #[must_use]
    pub fn opening_points(self) -> (f64, f64) {
        let (usable_width, usable_height) = self.usable_points();
        (
            PREFERRED_WIDTH.min(usable_width).max(MIN_WIDTH),
            PREFERRED_HEIGHT.min(usable_height).max(MIN_HEIGHT),
        )
    }
}

#[cfg(target_os = "windows")]
#[allow(unsafe_code, reason = "isolated Win32 work-area query")]
#[must_use]
pub fn primary_desktop(scale: f64) -> Option<Desktop> {
    use windows_sys::Win32::{
        Foundation::RECT,
        UI::WindowsAndMessaging::{SPI_GETWORKAREA, SystemParametersInfoW},
    };

    let mut area = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };

    let queried = unsafe {
        SystemParametersInfoW(SPI_GETWORKAREA, 0, std::ptr::from_mut(&mut area).cast(), 0)
    };
    if queried == 0 {
        return None;
    }
    let width = f64::from(area.right - area.left);
    let height = f64::from(area.bottom - area.top);
    (width > 0.0 && height > 0.0).then_some(Desktop {
        work_width: width,
        work_height: height,
        scale,
    })
}

#[cfg(not(target_os = "windows"))]
#[must_use]
pub fn primary_desktop(_scale: f64) -> Option<Desktop> {
    None
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartupPhase {
    Window,
    Font,
    Gpu,
    Preferences,
    Template,
    Workspace,
    Ready,
}

impl StartupPhase {
    #[cfg(test)]
    pub const ALL: [Self; 7] = [
        Self::Window,
        Self::Font,
        Self::Gpu,
        Self::Preferences,
        Self::Template,
        Self::Workspace,
        Self::Ready,
    ];

    #[must_use]
    pub const fn fraction(self) -> f32 {
        match self {
            Self::Window => 0.08,
            Self::Font => 0.18,
            Self::Gpu => 0.32,
            Self::Preferences => 0.58,
            Self::Template => 0.72,
            Self::Workspace => 0.90,
            Self::Ready => 1.0,
        }
    }

    #[must_use]
    pub const fn label(self, locale: Locale) -> &'static str {
        text(
            locale,
            match self {
                Self::Window => TextKey::BootStarting,
                Self::Font => TextKey::BootFonts,
                Self::Gpu => TextKey::BootGraphics,
                Self::Preferences => TextKey::BootSettings,
                Self::Template => TextKey::BootTemplate,
                Self::Workspace => TextKey::BootWorkspace,
                Self::Ready => TextKey::BootReady,
            },
        )
    }
}

#[must_use]
pub fn boot_locale(saved: Option<Locale>) -> Locale {
    saved.unwrap_or_else(Locale::system_default)
}

#[must_use]
pub fn boot_locale_from_settings() -> Locale {
    boot_locale(
        PreferenceStore::discover()
            .and_then(|store| store.load().ok())
            .and_then(|preferences| preferences.locale),
    )
}

#[must_use]
pub const fn boot_font_face(locale: Locale) -> &'static str {
    match locale {
        Locale::Korean => "Malgun Gothic",
        Locale::Japanese => "Yu Gothic UI",
        Locale::ZhHans => "Microsoft YaHei UI",
        Locale::ZhHant => "Microsoft JhengHei UI",
        Locale::Thai => "Leelawadee UI",
        Locale::Hindi | Locale::Bengali => "Nirmala UI",

        Locale::English
        | Locale::Spanish
        | Locale::Portuguese
        | Locale::French
        | Locale::German
        | Locale::Russian
        | Locale::Indonesian
        | Locale::Vietnamese => "Segoe UI",
    }
}

#[derive(Debug)]
pub struct GraphicsUnavailable {
    detail: String,
}

impl GraphicsUnavailable {
    pub fn new(detail: impl std::fmt::Display) -> Self {
        Self {
            detail: detail.to_string(),
        }
    }
}

impl std::fmt::Display for GraphicsUnavailable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "no DX12 adapter or device: {}", self.detail)
    }
}

impl std::error::Error for GraphicsUnavailable {}

pub fn report_startup_failure(error: &(dyn std::error::Error + 'static)) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static REPORTED: AtomicBool = AtomicBool::new(false);
    if REPORTED.swap(true, Ordering::SeqCst) {
        return;
    }

    let graphics = error.downcast_ref::<GraphicsUnavailable>().is_some();
    let detail = error.to_string();
    let _ = diagnostics::record(
        Severity::Error,
        "startup",
        if graphics {
            "graphics_unavailable"
        } else {
            "initialization_failed"
        },
        &detail,
    );

    let _ = diagnostics::flush();

    let locale = boot_locale_from_settings();
    let log_path = diagnostics::default_log_path().ok();
    let body = startup_failure_message(locale, graphics, log_path.as_deref(), &detail);
    show_fatal_dialog(text(locale, TextKey::StartupFailedTitle), &body);
}

fn startup_failure_message(
    locale: Locale,
    graphics: bool,
    log_path: Option<&Path>,
    detail: &str,
) -> String {
    let mut message = String::from(text(
        locale,
        if graphics {
            TextKey::StartupFailedGraphics
        } else {
            TextKey::StartupFailedOther
        },
    ));
    if let Some(path) = log_path {
        message.push_str("\n\n");
        message.push_str(text(locale, TextKey::StartupFailedLogHint));
        message.push('\n');
        message.push_str(&path.display().to_string());
    }
    let detail = detail.trim();
    if !detail.is_empty() {
        message.push_str("\n\n");
        message.push_str(detail);
    }
    message
}

#[cfg(target_os = "windows")]
#[allow(unsafe_code, reason = "isolated Win32 owner-less fatal message box")]
fn show_fatal_dialog(title: &str, body: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MB_TOPMOST, MessageBoxW,
    };

    fn wide(value: &str) -> Vec<u16> {
        value
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>()
    }

    let body = wide(body);
    let title = wide(title);

    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            body.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR | MB_SETFOREGROUND | MB_TOPMOST,
        );
    }
}

#[cfg(not(target_os = "windows"))]
fn show_fatal_dialog(title: &str, body: &str) {
    eprintln!("{title}\n{body}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_minimum_window_fits_the_desktops_people_actually_have() {
        let desktops = [
            (
                "1920x1080 @100%, Win11",
                1920.0,
                1080.0,
                1.0,
                WINDOWS_11_TASKBAR_POINTS,
            ),
            (
                "1920x1080 @125%, Win11",
                1920.0,
                1080.0,
                1.25,
                WINDOWS_11_TASKBAR_POINTS,
            ),
            (
                "1920x1080 @150%, Win11",
                1920.0,
                1080.0,
                1.5,
                WINDOWS_11_TASKBAR_POINTS,
            ),
            (
                "1920x1080 @150%, Win10",
                1920.0,
                1080.0,
                1.5,
                WINDOWS_10_TASKBAR_POINTS,
            ),
            (
                "1920x1080 @175%, Win11",
                1920.0,
                1080.0,
                1.75,
                WINDOWS_11_TASKBAR_POINTS,
            ),
            (
                "1600x900 @125%, Win11",
                1600.0,
                900.0,
                1.25,
                WINDOWS_11_TASKBAR_POINTS,
            ),
            (
                "1366x768 @100%, Win10",
                1366.0,
                768.0,
                1.0,
                WINDOWS_10_TASKBAR_POINTS,
            ),
            (
                "2560x1440 @150%, Win11",
                2560.0,
                1440.0,
                1.5,
                WINDOWS_11_TASKBAR_POINTS,
            ),
        ];
        for (name, width, height, scale, taskbar) in desktops {
            let desktop = Desktop::from_screen(width, height, scale, taskbar);
            let (usable_width, usable_height) = desktop.usable_points();
            assert!(
                desktop.fits_minimum_window(),
                "{name} leaves {usable_width}x{usable_height} points, \
                 which cannot show {MIN_WIDTH}x{MIN_HEIGHT}"
            );
        }
    }

    #[test]
    fn every_minimum_that_shipped_before_was_taller_than_some_1080p_desktop() {
        const FIRST_HEIGHT: f64 = 680.0;
        let at_150 = Desktop::from_screen(1920.0, 1080.0, 1.5, WINDOWS_11_TASKBAR_POINTS);
        let (usable_width, usable_height) = at_150.usable_points();
        assert_eq!((usable_width, usable_height), (1280.0, 672.0));
        assert!(FIRST_HEIGHT > usable_height, "the first minimum, at 150%");
        assert!(MIN_HEIGHT <= usable_height);

        assert!(usable_height - MIN_HEIGHT >= 32.0);
        assert!(usable_width - MIN_WIDTH >= 32.0);

        const SECOND_HEIGHT: f64 = 640.0;
        let at_175 = Desktop::from_screen(1920.0, 1080.0, 1.75, WINDOWS_11_TASKBAR_POINTS);
        let (usable_width, usable_height) = at_175.usable_points();
        assert!((usable_width - 1_097.14).abs() < 0.01, "{usable_width}");
        assert!((usable_height - 569.14).abs() < 0.01, "{usable_height}");
        assert!(SECOND_HEIGHT > usable_height, "the second minimum, at 175%");
        assert!(MIN_HEIGHT <= usable_height);
        assert!(usable_width - MIN_WIDTH >= 32.0);
    }

    #[test]
    fn the_window_opens_inside_the_work_area_rather_than_past_its_edge() {
        let small = Desktop::from_screen(1920.0, 1080.0, 1.5, WINDOWS_11_TASKBAR_POINTS);
        assert_eq!(small.opening_points(), (1280.0, 672.0));

        let large = Desktop::from_screen(3840.0, 2160.0, 1.0, WINDOWS_11_TASKBAR_POINTS);
        assert_eq!(large.opening_points(), (PREFERRED_WIDTH, PREFERRED_HEIGHT));

        let tiny = Desktop::from_screen(1024.0, 600.0, 1.0, WINDOWS_10_TASKBAR_POINTS);
        assert!(!tiny.fits_minimum_window());
        assert_eq!(tiny.opening_points(), (MIN_WIDTH, MIN_HEIGHT));
    }

    #[test]
    fn the_smaller_minimum_still_leaves_a_viewport_worth_having() {
        let chrome = f64::from(
            crate::theme::TOP_BAR_HEIGHT
                + crate::theme::STATUS_BAR_HEIGHT
                + crate::theme::PROGRESS_HEIGHT,
        );
        let content = MIN_HEIGHT - chrome;
        assert!(
            content >= 400.0,
            "{chrome} points of chrome leaves only {content} for the work"
        );

        let widest_inspector = f64::from(crate::theme::INSPECTOR_MAX_WIDTH);
        let viewport = MIN_WIDTH - widest_inspector;
        assert!(
            viewport >= f64::from(crate::theme::INSPECTOR_MIN_WIDTH),
            "an inspector at {widest_inspector} leaves a {viewport}-point viewport"
        );
    }

    #[test]
    fn a_broken_display_scale_is_read_as_one_hundred_percent() {
        for scale in [0.0, -1.5, f64::NAN, f64::INFINITY] {
            let desktop = Desktop {
                work_width: 1920.0,
                work_height: 1032.0,
                scale,
            };
            assert_eq!(desktop.usable_points(), (1920.0, 1032.0), "{scale}");
            assert!(desktop.fits_minimum_window());
        }
    }

    #[test]
    fn the_boot_screen_progress_only_moves_forward() {
        assert!(
            StartupPhase::ALL
                .windows(2)
                .all(|pair| pair[0].fraction() < pair[1].fraction())
        );
        assert_eq!(StartupPhase::Ready.fraction(), 1.0);
    }

    #[test]
    fn the_boot_screen_speaks_every_language_the_app_does() {
        for locale in Locale::ALL {
            for phase in StartupPhase::ALL {
                let label = phase.label(locale);
                assert!(!label.trim().is_empty(), "{locale:?} {phase:?}");
            }

            if locale != Locale::Korean {
                assert_ne!(
                    StartupPhase::Window.label(locale),
                    StartupPhase::Window.label(Locale::Korean),
                    "{locale:?} still reads as Korean"
                );
            }
            assert!(!boot_font_face(locale).is_empty());
        }
    }

    #[test]
    fn a_saved_language_decides_the_boot_screen() {
        assert_eq!(boot_locale(Some(Locale::Thai)), Locale::Thai);
        assert_eq!(boot_locale(Some(Locale::Korean)), Locale::Korean);
        assert_eq!(boot_locale_from_settings(), boot_locale_from_settings());
    }

    #[test]
    fn the_fatal_dialog_names_the_cause_the_log_and_the_detail() {
        let log = Path::new(r"C:\Users\somebody\AppData\Local\Vkit\logs\vkit.log");
        let message = startup_failure_message(
            Locale::English,
            true,
            Some(log),
            "RequestAdapterError: no adapters",
        );
        assert!(message.contains("DirectX 12"), "{message}");
        assert!(message.contains("vkit.log"), "{message}");
        assert!(message.contains("RequestAdapterError"), "{message}");

        for locale in Locale::ALL {
            for graphics in [true, false] {
                let message =
                    startup_failure_message(locale, graphics, Some(log), "device request failed");
                assert!(message.contains("Vkit"), "{locale:?}: {message}");
                assert!(message.contains("vkit.log"), "{locale:?}: {message}");
                assert!(
                    message.contains("device request failed"),
                    "{locale:?}: {message}"
                );
                assert!(!text(locale, TextKey::StartupFailedTitle).trim().is_empty());
            }
        }
    }

    #[test]
    fn a_dialog_with_nowhere_to_point_omits_the_line_rather_than_dangling() {
        let message = startup_failure_message(Locale::English, true, None, "");
        assert_eq!(
            message,
            text(Locale::English, TextKey::StartupFailedGraphics)
        );
        assert!(!message.ends_with(':'));
    }

    #[test]
    fn only_a_graphics_failure_is_reported_as_one() {
        let graphics = GraphicsUnavailable::new("adapter request failed");
        let error: &(dyn std::error::Error + 'static) = &graphics;
        assert!(error.downcast_ref::<GraphicsUnavailable>().is_some());
        assert!(error.to_string().contains("adapter request failed"));

        let other = std::io::Error::other("the settings directory is read-only");
        let error: &(dyn std::error::Error + 'static) = &other;
        assert!(error.downcast_ref::<GraphicsUnavailable>().is_none());
    }
}

#[cfg(test)]
mod locale_boot {
    use super::boot_locale;
    use crate::i18n::Locale;

    #[test]
    fn the_machine_is_asked_once_and_the_answer_is_kept() {
        assert_eq!(
            boot_locale(Some(Locale::Korean)),
            Locale::Korean,
            "a language already chosen is never overruled by the machine",
        );
        assert_eq!(
            boot_locale(Some(Locale::English)),
            Locale::English,
            "English chosen deliberately must not be mistaken for the fallback",
        );
        assert_eq!(
            boot_locale(None),
            Locale::system_default(),
            "only a first run has nothing to go on",
        );
    }
}
