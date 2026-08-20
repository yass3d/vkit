use std::path::Path;

pub(crate) const THUMBNAIL_SIDE: u32 = 512;

#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "plain GDI screen-capture sequence; every handle is released on every path"
)]
pub(crate) fn capture_screen_square(
    hwnd: windows_sys::Win32::Foundation::HWND,
    square_points: [f32; 4],
    pixels_per_point: f32,
) -> Result<image::RgbaImage, String> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, ClientToScreen,
        CreateCompatibleBitmap, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SRCCOPY, SelectObject,
    };

    let mut origin = POINT { x: 0, y: 0 };
    if unsafe { ClientToScreen(hwnd, &mut origin) } == 0 {
        return Err("the window's client origin is unavailable".to_owned());
    }
    let scale = pixels_per_point.max(0.1);
    let x = origin.x + (square_points[0] * scale).round() as i32;
    let y = origin.y + (square_points[1] * scale).round() as i32;
    let side = (((square_points[2] - square_points[0]) * scale).round() as i32).max(1);

    unsafe {
        let screen = GetDC(std::ptr::null_mut());
        if screen.is_null() {
            return Err("cannot open the screen device context".to_owned());
        }
        let memory = CreateCompatibleDC(screen);
        let bitmap = CreateCompatibleBitmap(screen, side, side);
        let previous = SelectObject(memory, bitmap as _);
        let copied = BitBlt(memory, 0, 0, side, side, screen, x, y, SRCCOPY | CAPTUREBLT);

        let mut pixels = vec![0_u8; side as usize * side as usize * 4];
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = side;
        info.bmiHeader.biHeight = -side;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let rows = GetDIBits(
            memory,
            bitmap,
            0,
            side as u32,
            pixels.as_mut_ptr().cast(),
            &mut info,
            DIB_RGB_COLORS,
        );

        SelectObject(memory, previous);
        DeleteObject(bitmap as _);
        DeleteDC(memory);
        ReleaseDC(std::ptr::null_mut(), screen);

        if copied == 0 || rows == 0 {
            return Err("the screen copy failed".to_owned());
        }
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        image::RgbaImage::from_raw(side as u32, side as u32, pixels)
            .ok_or_else(|| "the captured buffer has the wrong size".to_owned())
    }
}

pub(crate) fn encode_square_jpeg(square: &image::RgbaImage, side: u32) -> Result<Vec<u8>, String> {
    if square.width() == 0 || square.height() == 0 {
        return Err("empty capture".to_owned());
    }
    let scaled = image::imageops::resize(square, side, side, image::imageops::FilterType::Lanczos3);
    let rgb = image::DynamicImage::ImageRgba8(scaled).to_rgb8();

    let mut encoded = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 90);
    encoder
        .encode(rgb.as_raw(), side, side, image::ExtendedColorType::Rgb8)
        .map_err(|err| format!("thumbnail encode failed: {err}"))?;
    Ok(encoded)
}

pub(crate) fn write_file(path: &Path, encoded: &[u8]) -> Result<(), String> {
    std::fs::write(path, encoded).map_err(|err| format!("cannot write {}: {err}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(side: u32) -> image::RgbaImage {
        image::RgbaImage::from_fn(side, side, |x, y| image::Rgba([x as u8, y as u8, 128, 255]))
    }

    #[test]
    fn a_capture_encodes_square_at_the_asked_side_and_fans_out() {
        let directory = tempfile::tempdir().expect("tempdir");
        let first = directory.path().join("style.jpg");
        let second = directory.path().join("Preset_style.jpg");
        let encoded = encode_square_jpeg(&gradient(200), 64).expect("encodes");
        for path in [first, second] {
            write_file(&path, &encoded).expect("writes");
            let saved = image::open(&path).expect("thumbnail decodes");
            assert_eq!((saved.width(), saved.height()), (64, 64), "{path:?}");
        }
    }

    #[test]
    fn an_empty_capture_refuses_rather_than_panics() {
        let empty = image::RgbaImage::new(0, 0);
        assert!(encode_square_jpeg(&empty, 64).is_err());
    }
}
