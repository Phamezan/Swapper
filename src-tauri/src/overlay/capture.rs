//! Screen pixels shared by the HUD arrow and scoreboard finders.
use std::ptr::null_mut;
use windows_sys::Win32::Graphics::Gdi::*;

pub fn capture(x: i32, y: i32, width: i32, height: i32) -> Option<Vec<u32>> {
    let length = usize::try_from(width).ok()?.checked_mul(usize::try_from(height).ok()?)?;
    if length == 0 { return None; }
    unsafe {
        let screen = GetDC(null_mut());
        if screen.is_null() {
            return None;
        }
        let dc = CreateCompatibleDC(screen);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        let mut result = None;
        if !dc.is_null() && !bitmap.is_null() && !bits.is_null() {
            let previous = SelectObject(dc, bitmap);
            if BitBlt(dc, 0, 0, width, height, screen, x, y, SRCCOPY) != 0 {
                result = Some(
                    std::slice::from_raw_parts(bits.cast::<u32>(), length)
                        .to_vec(),
                );
            }
            SelectObject(dc, previous);
        }
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        if !dc.is_null() {
            DeleteDC(dc);
        }
        ReleaseDC(null_mut(), screen);
        result
    }
}

/// Test-only decoding through the same native graphics library as the renderer.
#[cfg(test)]
pub(super) fn load_png(path: &std::path::Path) -> (Vec<u32>, usize, usize) {
    use windows_sys::Win32::Graphics::GdiPlus::*;
    unsafe {
        let mut token = 0;
        let input = GdiplusStartupInput { GdiplusVersion: 1, ..Default::default() };
        assert_eq!(GdiplusStartup(&mut token, &input, null_mut()), Ok);
        let mut image = null_mut();
        let path: Vec<u16> = path.to_string_lossy().encode_utf16().chain(Some(0)).collect();
        assert_eq!(GdipCreateBitmapFromFile(path.as_ptr(), &mut image), Ok);
        let (mut width, mut height) = (0, 0);
        assert_eq!(GdipGetImageWidth(image.cast(), &mut width), Ok);
        assert_eq!(GdipGetImageHeight(image.cast(), &mut height), Ok);
        let rect = Rect { X: 0, Y: 0, Width: width as i32, Height: height as i32 };
        let mut data = BitmapData::default();
        assert_eq!(GdipBitmapLockBits(image, &rect, ImageLockModeRead as u32, 0x0026200A, &mut data), Ok);
        let mut pixels = Vec::with_capacity(width as usize * height as usize);
        for y in 0..height as isize {
            let row = data.Scan0.cast::<u8>().offset(y * data.Stride as isize).cast::<u32>();
            pixels.extend_from_slice(std::slice::from_raw_parts(row, width as usize));
        }
        assert_eq!(GdipBitmapUnlockBits(image, &mut data), Ok);
        GdipDisposeImage(image.cast());
        GdiplusShutdown(token);
        (pixels, width as usize, height as usize)
    }
}
