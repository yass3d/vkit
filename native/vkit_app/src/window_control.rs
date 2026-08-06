use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU8, Ordering},
};

use egui::viewport::ResizeDirection as EguiResizeDirection;
use winit::window::Window;

pub(crate) const HT_CLIENT: u32 = 1;
pub(crate) const HT_CAPTION: u32 = 2;
pub(crate) const HT_MIN_BUTTON: u32 = 8;
pub(crate) const HT_MAX_BUTTON: u32 = 9;
pub(crate) const HT_LEFT: u32 = 10;
pub(crate) const HT_RIGHT: u32 = 11;
pub(crate) const HT_TOP: u32 = 12;
pub(crate) const HT_TOP_LEFT: u32 = 13;
pub(crate) const HT_TOP_RIGHT: u32 = 14;
pub(crate) const HT_BOTTOM: u32 = 15;
pub(crate) const HT_BOTTOM_LEFT: u32 = 16;
pub(crate) const HT_BOTTOM_RIGHT: u32 = 17;
pub(crate) const HT_CLOSE: u32 = 20;

pub(crate) const RESIZE_BAND_POINTS: f32 = 8.0;
pub(crate) const RESIZE_CORNER_POINTS: f32 = 16.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct NcRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl NcRect {
    pub(crate) const ZERO: Self = Self {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };

    pub(crate) fn from_egui(rect: egui::Rect) -> Self {
        Self {
            left: rect.left(),
            top: rect.top(),
            right: rect.right(),
            bottom: rect.bottom(),
        }
    }

    fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NcLayout {
    pub pixels_per_point: f32,

    pub titlebar_height: f32,

    pub caption_buttons: [NcRect; 3],

    pub carve_outs: Vec<NcRect>,
}

impl NcLayout {
    const fn empty() -> Self {
        Self {
            pixels_per_point: 0.0,
            titlebar_height: 0.0,
            caption_buttons: [NcRect::ZERO; 3],
            carve_outs: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CaptionButton {
    Minimize,
    Maximize,
    Close,
}

impl CaptionButton {
    pub(crate) const ALL: [Self; 3] = [Self::Minimize, Self::Maximize, Self::Close];

    pub(crate) const fn from_hit(code: u32) -> Option<Self> {
        match code {
            HT_MIN_BUTTON => Some(Self::Minimize),
            HT_MAX_BUTTON => Some(Self::Maximize),
            HT_CLOSE => Some(Self::Close),
            _ => None,
        }
    }

    const fn hit_code(self) -> u32 {
        match self {
            Self::Minimize => HT_MIN_BUTTON,
            Self::Maximize => HT_MAX_BUTTON,
            Self::Close => HT_CLOSE,
        }
    }

    const fn as_slot(self) -> u8 {
        match self {
            Self::Minimize => 1,
            Self::Maximize => 2,
            Self::Close => 3,
        }
    }

    const fn from_slot(slot: u8) -> Option<Self> {
        match slot {
            1 => Some(Self::Minimize),
            2 => Some(Self::Maximize),
            3 => Some(Self::Close),
            _ => None,
        }
    }
}

static NC_LAYOUT: Mutex<NcLayout> = Mutex::new(NcLayout::empty());
static NC_SUBCLASS_ACTIVE: AtomicBool = AtomicBool::new(false);
static NC_HOVERED: AtomicU8 = AtomicU8::new(0);
static NC_PRESSED: AtomicU8 = AtomicU8::new(0);
static NC_SYNTHETIC_RELEASE: AtomicBool = AtomicBool::new(false);

pub(crate) fn nc_subclass_active() -> bool {
    NC_SUBCLASS_ACTIVE.load(Ordering::Acquire)
}

pub(crate) fn publish_nc_layout(layout: NcLayout) {
    let mut guard = match NC_LAYOUT.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    *guard = layout;
}

fn snapshot_nc_layout() -> NcLayout {
    match NC_LAYOUT.lock() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

pub(crate) fn hovered_caption_button() -> Option<CaptionButton> {
    CaptionButton::from_slot(NC_HOVERED.load(Ordering::Relaxed))
}

pub(crate) fn pressed_caption_button() -> Option<CaptionButton> {
    CaptionButton::from_slot(NC_PRESSED.load(Ordering::Relaxed))
}

pub(crate) fn take_synthetic_pointer_release() -> bool {
    NC_SYNTHETIC_RELEASE.swap(false, Ordering::AcqRel)
}

fn store_caption_slot(cell: &AtomicU8, button: Option<CaptionButton>) -> bool {
    let next = button.map_or(0, CaptionButton::as_slot);
    cell.swap(next, Ordering::Relaxed) != next
}

pub(crate) fn hit_test_client_point(
    layout: &NcLayout,
    client_width_px: f32,
    client_height_px: f32,
    x_px: f32,
    y_px: f32,
    maximized: bool,
) -> u32 {
    let scale = if layout.pixels_per_point > f32::EPSILON {
        layout.pixels_per_point
    } else {
        1.0
    };
    let x = x_px / scale;
    let y = y_px / scale;
    let width = client_width_px / scale;
    let height = client_height_px / scale;

    if !maximized && let Some(code) = resize_band_hit(x, y, width, height) {
        return code;
    }

    for button in CaptionButton::ALL {
        let mut cell = layout.caption_buttons[button as usize];
        if maximized {
            cell.top = 0.0;
        }
        if cell.contains(x, y) {
            return button.hit_code();
        }
    }

    for carve_out in &layout.carve_outs {
        if carve_out.contains(x, y) {
            return HT_CLIENT;
        }
    }

    if y < layout.titlebar_height {
        return HT_CAPTION;
    }

    HT_CLIENT
}

fn resize_band_hit(x: f32, y: f32, width: f32, height: f32) -> Option<u32> {
    let band = RESIZE_BAND_POINTS;
    let corner = RESIZE_CORNER_POINTS;
    if width < corner * 2.0 || height < corner * 2.0 {
        return None;
    }
    let on_left = x < band;
    let on_right = x >= width - band;
    let on_top = y < band;
    let on_bottom = y >= height - band;
    if !(on_left || on_right || on_top || on_bottom) {
        return None;
    }

    let near_left = x < corner;
    let near_right = x >= width - corner;
    let near_top = y < corner;
    let near_bottom = y >= height - corner;
    if near_left && near_top {
        return Some(HT_TOP_LEFT);
    }
    if near_right && near_top {
        return Some(HT_TOP_RIGHT);
    }
    if near_left && near_bottom {
        return Some(HT_BOTTOM_LEFT);
    }
    if near_right && near_bottom {
        return Some(HT_BOTTOM_RIGHT);
    }
    if on_top {
        return Some(HT_TOP);
    }
    if on_bottom {
        return Some(HT_BOTTOM);
    }
    if on_left {
        return Some(HT_LEFT);
    }
    Some(HT_RIGHT)
}

pub(crate) fn begin_titlebar_drag(window: &Window) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        post_windows_non_client_drag(window, HT_CAPTION)
    }
    #[cfg(not(target_os = "windows"))]
    window.drag_window().map_err(|error| error.to_string())
}

pub(crate) fn begin_window_resize(
    window: &Window,
    direction: EguiResizeDirection,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        post_windows_non_client_drag(window, windows_resize_hit_test(direction))
    }
    #[cfg(not(target_os = "windows"))]
    window
        .drag_resize_window(winit_resize_direction(direction))
        .map_err(|error| error.to_string())
}

#[cfg(not(target_os = "windows"))]
fn winit_resize_direction(direction: EguiResizeDirection) -> winit::window::ResizeDirection {
    use winit::window::ResizeDirection as WinitResizeDirection;

    match direction {
        EguiResizeDirection::North => WinitResizeDirection::North,
        EguiResizeDirection::South => WinitResizeDirection::South,
        EguiResizeDirection::East => WinitResizeDirection::East,
        EguiResizeDirection::West => WinitResizeDirection::West,
        EguiResizeDirection::NorthEast => WinitResizeDirection::NorthEast,
        EguiResizeDirection::SouthEast => WinitResizeDirection::SouthEast,
        EguiResizeDirection::NorthWest => WinitResizeDirection::NorthWest,
        EguiResizeDirection::SouthWest => WinitResizeDirection::SouthWest,
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn windows_hwnd(
    window: &Window,
) -> Result<windows_sys::Win32::Foundation::HWND, String> {
    use winit::raw_window_handle::{HasWindowHandle as _, RawWindowHandle};

    let handle = window
        .window_handle()
        .map_err(|error| format!("window handle unavailable: {error}"))?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err("window is not backed by a Win32 HWND".to_owned());
    };
    let hwnd = handle.hwnd.get() as windows_sys::Win32::Foundation::HWND;
    if hwnd.is_null() {
        Err("window returned a null HWND".to_owned())
    } else {
        Ok(hwnd)
    }
}

#[cfg(target_os = "windows")]
const fn windows_resize_hit_test(direction: EguiResizeDirection) -> u32 {
    match direction {
        EguiResizeDirection::North => HT_TOP,
        EguiResizeDirection::South => HT_BOTTOM,
        EguiResizeDirection::East => HT_RIGHT,
        EguiResizeDirection::West => HT_LEFT,
        EguiResizeDirection::NorthEast => HT_TOP_RIGHT,
        EguiResizeDirection::SouthEast => HT_BOTTOM_RIGHT,
        EguiResizeDirection::NorthWest => HT_TOP_LEFT,
        EguiResizeDirection::SouthWest => HT_BOTTOM_LEFT,
    }
}

#[cfg(target_os = "windows")]
const fn screen_point_lparam(x: i32, y: i32) -> isize {
    ((x as u16 as u32) | ((y as u16 as u32) << 16)) as isize
}

#[cfg(target_os = "windows")]
const NC_SUBCLASS_ID: usize = 0xFACE;

#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "isolated comctl32 subclass installation on the live main HWND"
)]
pub(crate) fn install_window_subclass(window: &Window) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::SetWindowSubclass;

    let hwnd = windows_hwnd(window)?;

    if unsafe { SetWindowSubclass(hwnd, Some(nc_subclass_proc), NC_SUBCLASS_ID, 0) } == 0 {
        return Err(format!(
            "SetWindowSubclass failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    NC_SUBCLASS_ACTIVE.store(true, Ordering::Release);
    Ok(())
}

#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "Win32 subclass procedure; every call is documented at the site"
)]
unsafe extern "system" fn nc_subclass_proc(
    hwnd: windows_sys::Win32::Foundation::HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
    _subclass_id: usize,
    _ref_data: usize,
) -> isize {
    use windows_sys::Win32::UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass},
        WindowsAndMessaging::{
            WM_CAPTURECHANGED, WM_EXITSIZEMOVE, WM_MOUSEMOVE, WM_NCDESTROY, WM_NCHITTEST,
            WM_NCLBUTTONDBLCLK, WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCMOUSELEAVE, WM_NCMOUSEMOVE,
        },
    };

    let chain = || unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };

    match message {
        WM_NCHITTEST => match non_client_hit_for_screen_point(hwnd, lparam) {
            Some(code) => code as isize,
            None => chain(),
        },
        WM_NCMOUSEMOVE => {
            if store_caption_slot(&NC_HOVERED, CaptionButton::from_hit(wparam as u32)) {
                request_native_redraw(hwnd);
            }
            track_non_client_mouse_leave(hwnd);
            chain()
        }
        WM_MOUSEMOVE => {
            let hover_cleared = store_caption_slot(&NC_HOVERED, None);
            let press_cleared = store_caption_slot(&NC_PRESSED, None);
            if hover_cleared || press_cleared {
                request_native_redraw(hwnd);
            }
            chain()
        }
        WM_NCMOUSELEAVE => {
            let hover_cleared = store_caption_slot(&NC_HOVERED, None);
            let press_cleared = store_caption_slot(&NC_PRESSED, None);
            if hover_cleared || press_cleared {
                request_native_redraw(hwnd);
            }
            chain()
        }
        WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK => {
            if let Some(button) = CaptionButton::from_hit(wparam as u32) {
                store_caption_slot(&NC_PRESSED, Some(button));
                request_native_redraw(hwnd);
                return 0;
            }
            chain()
        }
        WM_NCLBUTTONUP => {
            let pressed = pressed_caption_button();
            if let Some(button) = pressed {
                store_caption_slot(&NC_PRESSED, None);
                request_native_redraw(hwnd);
                if CaptionButton::from_hit(wparam as u32) == Some(button) {
                    post_caption_button_command(hwnd, button);
                }
                return 0;
            }
            chain()
        }
        WM_EXITSIZEMOVE => {
            NC_SYNTHETIC_RELEASE.store(true, Ordering::Release);
            store_caption_slot(&NC_PRESSED, None);
            request_native_redraw(hwnd);
            chain()
        }
        WM_CAPTURECHANGED => {
            if lparam != hwnd as isize {
                let press_cleared = store_caption_slot(&NC_PRESSED, None);
                if press_cleared {
                    NC_SYNTHETIC_RELEASE.store(true, Ordering::Release);
                }
            }
            chain()
        }
        WM_NCDESTROY => {
            NC_SUBCLASS_ACTIVE.store(false, Ordering::Release);

            unsafe {
                RemoveWindowSubclass(hwnd, Some(nc_subclass_proc), NC_SUBCLASS_ID);
            }
            chain()
        }
        _ => chain(),
    }
}

#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "reads live window geometry for the subclass hit test"
)]
fn non_client_hit_for_screen_point(
    hwnd: windows_sys::Win32::Foundation::HWND,
    lparam: isize,
) -> Option<u32> {
    use windows_sys::Win32::{
        Foundation::{POINT, RECT},
        Graphics::Gdi::ScreenToClient,
        UI::WindowsAndMessaging::{GetClientRect, IsZoomed},
    };

    let mut point = POINT {
        x: (lparam & 0xFFFF) as u16 as i16 as i32,
        y: ((lparam >> 16) & 0xFFFF) as u16 as i16 as i32,
    };

    if unsafe { ScreenToClient(hwnd, &mut point) } == 0 {
        return None;
    }
    let mut client = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };

    if unsafe { GetClientRect(hwnd, &mut client) } == 0 {
        return None;
    }

    let maximized = unsafe { IsZoomed(hwnd) } != 0;

    let layout = snapshot_nc_layout();
    Some(hit_test_client_point(
        &layout,
        (client.right - client.left) as f32,
        (client.bottom - client.top) as f32,
        point.x as f32,
        point.y as f32,
        maximized,
    ))
}

#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "RedrawWindow(RDW_INTERNALPAINT) is winit's own repaint mechanism"
)]
fn request_native_redraw(hwnd: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Graphics::Gdi::{RDW_INTERNALPAINT, RedrawWindow};

    unsafe {
        RedrawWindow(
            hwnd,
            std::ptr::null(),
            std::ptr::null_mut(),
            RDW_INTERNALPAINT,
        );
    }
}

#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "TrackMouseEvent registration for non-client leave notifications"
)]
fn track_non_client_mouse_leave(hwnd: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent,
    };

    let mut track = TRACKMOUSEEVENT {
        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE | TME_NONCLIENT,
        hwndTrack: hwnd,
        dwHoverTime: 0,
    };

    unsafe {
        TrackMouseEvent(&mut track);
    }
}

#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "posts standard system commands to the live main HWND"
)]
fn post_caption_button_command(hwnd: windows_sys::Win32::Foundation::HWND, button: CaptionButton) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsZoomed, PostMessageW, SC_MAXIMIZE, SC_MINIMIZE, SC_RESTORE, WM_CLOSE, WM_SYSCOMMAND,
    };

    let (message, wparam) = match button {
        CaptionButton::Minimize => (WM_SYSCOMMAND, SC_MINIMIZE as usize),
        CaptionButton::Maximize => {
            let maximized = unsafe { IsZoomed(hwnd) } != 0;
            let command = if maximized { SC_RESTORE } else { SC_MAXIMIZE };
            (WM_SYSCOMMAND, command as usize)
        }
        CaptionButton::Close => (WM_CLOSE, 0),
    };

    unsafe {
        PostMessageW(hwnd, message, wparam, 0);
    }
}

#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "isolated asynchronous Win32 non-client drag bridge"
)]
fn post_windows_non_client_drag(window: &Window, hit_test: u32) -> Result<(), String> {
    use windows_sys::Win32::{
        Foundation::POINT,
        UI::{
            Input::KeyboardAndMouse::{GetKeyState, ReleaseCapture, VK_LBUTTON},
            WindowsAndMessaging::{GetCursorPos, PostMessageW, WM_NCLBUTTONDOWN},
        },
    };

    let hwnd = windows_hwnd(window)?;

    if unsafe { GetKeyState(VK_LBUTTON as i32) } >= 0 {
        let _ = crate::diagnostics::record(
            crate::diagnostics::Severity::Debug,
            "window",
            "legacy_drag_skipped",
            "primary button released before the non-client drag was posted",
        );
        return Ok(());
    }
    let mut point = POINT { x: 0, y: 0 };

    if unsafe { GetCursorPos(&mut point) } == 0 {
        return Err(format!(
            "GetCursorPos failed: {}",
            std::io::Error::last_os_error()
        ));
    }

    unsafe {
        ReleaseCapture();
    }

    if unsafe {
        PostMessageW(
            hwnd,
            WM_NCLBUTTONDOWN,
            hit_test as usize,
            screen_point_lparam(point.x, point.y),
        )
    } == 0
    {
        return Err(format!(
            "PostMessageW(WM_NCLBUTTONDOWN) failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: f32 = 1_600.0;
    const HEIGHT: f32 = 900.0;

    fn layout(scale: f32) -> NcLayout {
        NcLayout {
            pixels_per_point: scale,
            titlebar_height: 44.0,
            caption_buttons: [
                NcRect {
                    left: 1_480.0,
                    top: 6.0,
                    right: 1_520.0,
                    bottom: 38.0,
                },
                NcRect {
                    left: 1_520.0,
                    top: 6.0,
                    right: 1_560.0,
                    bottom: 38.0,
                },
                NcRect {
                    left: 1_560.0,
                    top: 6.0,
                    right: 1_600.0,
                    bottom: 38.0,
                },
            ],
            carve_outs: vec![
                NcRect {
                    left: 400.0,
                    top: 6.0,
                    right: 512.0,
                    bottom: 38.0,
                },
                NcRect {
                    left: 516.0,
                    top: 6.0,
                    right: 628.0,
                    bottom: 38.0,
                },
            ],
        }
    }

    fn hit(x: f32, y: f32, maximized: bool) -> u32 {
        hit_test_client_point(&layout(1.0), WIDTH, HEIGHT, x, y, maximized)
    }

    #[test]
    fn resize_edges_report_native_codes() {
        assert_eq!(hit(2.0, 300.0, false), HT_LEFT);
        assert_eq!(hit(1_598.0, 300.0, false), HT_RIGHT);
        assert_eq!(hit(800.0, 2.0, false), HT_TOP);
        assert_eq!(hit(800.0, 898.0, false), HT_BOTTOM);
    }

    #[test]
    fn corner_squares_extend_beyond_the_edge_bands() {
        assert_eq!(hit(2.0, 12.0, false), HT_TOP_LEFT);
        assert_eq!(hit(12.0, 2.0, false), HT_TOP_LEFT);
        assert_eq!(hit(1_590.0, 4.0, false), HT_TOP_RIGHT);
        assert_eq!(hit(4.0, 890.0, false), HT_BOTTOM_LEFT);
        assert_eq!(hit(1_594.0, 894.0, false), HT_BOTTOM_RIGHT);

        assert_eq!(hit(12.0, 12.0, false), HT_CAPTION);
    }

    #[test]
    fn titlebar_band_drags_including_gaps_between_widgets() {
        assert_eq!(hit(200.0, 20.0, false), HT_CAPTION, "brand area drags");
        assert_eq!(hit(514.0, 20.0, false), HT_CAPTION, "gap between capsules");
        assert_eq!(hit(700.0, 40.0, false), HT_CAPTION, "strip below widgets");
    }

    #[test]
    fn interactive_carve_outs_stay_client() {
        assert_eq!(hit(450.0, 20.0, false), HT_CLIENT);
        assert_eq!(hit(600.0, 20.0, false), HT_CLIENT);
    }

    #[test]
    fn caption_buttons_report_native_button_codes() {
        assert_eq!(hit(1_500.0, 20.0, false), HT_MIN_BUTTON);
        assert_eq!(hit(1_540.0, 20.0, false), HT_MAX_BUTTON);
        assert_eq!(hit(1_580.0, 20.0, false), HT_CLOSE);
    }

    #[test]
    fn resize_band_wins_over_buttons_when_windowed() {
        assert_eq!(hit(1_540.0, 4.0, false), HT_TOP);
    }

    #[test]
    fn maximized_skips_resize_bands_and_extends_buttons_to_the_top() {
        assert_eq!(hit(1_580.0, 2.0, true), HT_CLOSE);
        assert_eq!(hit(1_540.0, 2.0, true), HT_MAX_BUTTON);
        assert_eq!(hit(800.0, 2.0, true), HT_CAPTION);
        assert_eq!(hit(2.0, 300.0, true), HT_CLIENT);
        assert_eq!(hit(800.0, 898.0, true), HT_CLIENT);
    }

    #[test]
    fn below_the_titlebar_is_always_client() {
        assert_eq!(hit(800.0, 100.0, false), HT_CLIENT);
        assert_eq!(hit(800.0, 100.0, true), HT_CLIENT);
    }

    #[test]
    fn hit_testing_scales_with_the_dpi_factor() {
        let layout = layout(2.0);
        let physical = |x: f32, y: f32, maximized: bool| {
            hit_test_client_point(&layout, WIDTH * 2.0, HEIGHT * 2.0, x, y, maximized)
        };
        assert_eq!(physical(3_000.0, 40.0, false), HT_MIN_BUTTON);
        assert_eq!(physical(4.0, 600.0, false), HT_LEFT);

        assert_eq!(physical(15.0, 600.0, false), HT_LEFT);
        assert_eq!(physical(17.0, 600.0, false), HT_CLIENT);

        assert_eq!(physical(17.0, 40.0, false), HT_CAPTION);

        assert_eq!(physical(900.0, 40.0, false), HT_CLIENT);
    }

    #[test]
    fn unpublished_layout_keeps_the_window_usable() {
        let layout = NcLayout::empty();

        assert_eq!(
            hit_test_client_point(&layout, WIDTH, HEIGHT, 800.0, 2.0, false),
            HT_TOP
        );

        assert_eq!(
            hit_test_client_point(&layout, WIDTH, HEIGHT, 800.0, 20.0, false),
            HT_CLIENT
        );
    }

    #[test]
    fn tiny_windows_disable_resize_bands_instead_of_swallowing_the_ui() {
        let layout = layout(1.0);

        assert_eq!(
            hit_test_client_point(&layout, 20.0, 20.0, 2.0, 10.0, false),
            HT_CAPTION
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn legacy_resize_directions_map_to_the_matching_hit_codes() {
        use egui::viewport::ResizeDirection as EguiDirection;

        assert_eq!(windows_resize_hit_test(EguiDirection::North), HT_TOP);
        assert_eq!(windows_resize_hit_test(EguiDirection::South), HT_BOTTOM);
        assert_eq!(windows_resize_hit_test(EguiDirection::East), HT_RIGHT);
        assert_eq!(windows_resize_hit_test(EguiDirection::West), HT_LEFT);
        assert_eq!(
            windows_resize_hit_test(EguiDirection::NorthEast),
            HT_TOP_RIGHT
        );
        assert_eq!(
            windows_resize_hit_test(EguiDirection::SouthEast),
            HT_BOTTOM_RIGHT
        );
        assert_eq!(
            windows_resize_hit_test(EguiDirection::NorthWest),
            HT_TOP_LEFT
        );
        assert_eq!(
            windows_resize_hit_test(EguiDirection::SouthWest),
            HT_BOTTOM_LEFT
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn legacy_resize_directions_map_to_the_matching_winit_edges() {
        use egui::viewport::ResizeDirection as EguiDirection;
        use winit::window::ResizeDirection as WinitDirection;

        let pairs = [
            (EguiDirection::North, WinitDirection::North),
            (EguiDirection::South, WinitDirection::South),
            (EguiDirection::East, WinitDirection::East),
            (EguiDirection::West, WinitDirection::West),
            (EguiDirection::NorthEast, WinitDirection::NorthEast),
            (EguiDirection::SouthEast, WinitDirection::SouthEast),
            (EguiDirection::NorthWest, WinitDirection::NorthWest),
            (EguiDirection::SouthWest, WinitDirection::SouthWest),
        ];
        for (egui_direction, winit_direction) in pairs {
            assert_eq!(winit_resize_direction(egui_direction), winit_direction);
        }
    }

    #[test]
    fn caption_button_hit_codes_round_trip() {
        for button in CaptionButton::ALL {
            assert_eq!(CaptionButton::from_hit(button.hit_code()), Some(button));
            assert_eq!(CaptionButton::from_slot(button.as_slot()), Some(button));
        }
        assert_eq!(CaptionButton::from_hit(HT_CAPTION), None);
        assert_eq!(CaptionButton::from_slot(0), None);
    }
}
