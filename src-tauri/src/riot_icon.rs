//! The installed Riot Client's own icon, read from its executable the way
//! Explorer shows it. Swapper ships no Riot artwork.
use base64::Engine;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::null_mut;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Shell::SHDefExtractIconW;
use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

#[derive(serde::Serialize)]
pub struct IconPixels {
    pub width: u32,
    pub height: u32,
    /// Straight RGBA, base64; the page paints it to a canvas.
    pub rgba: String,
}

pub fn icon(path: &Path, size: u32) -> Option<IconPixels> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    unsafe {
        let mut large: HICON = null_mut();
        if SHDefExtractIconW(wide.as_ptr(), 0, 0, &mut large, null_mut(), size) != 0 || large.is_null() {
            return None;
        }
        let pixels = pixels(large);
        DestroyIcon(large);
        let (width, height, mut bgra) = pixels?;
        for px in bgra.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        Some(IconPixels {
            width,
            height,
            rgba: base64::engine::general_purpose::STANDARD.encode(bgra),
        })
    }
}

unsafe fn pixels(icon: HICON) -> Option<(u32, u32, Vec<u8>)> {
    let mut info: ICONINFO = std::mem::zeroed();
    if GetIconInfo(icon, &mut info) == 0 {
        return None;
    }
    let mut result = None;
    let mut bitmap: BITMAP = std::mem::zeroed();
    if !info.hbmColor.is_null()
        && GetObjectW(info.hbmColor, size_of::<BITMAP>() as i32, (&mut bitmap as *mut BITMAP).cast()) != 0
    {
        let (width, height) = (bitmap.bmWidth, bitmap.bmHeight);
        let mut header = BITMAPINFO {
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
        let mut buffer = vec![0u8; (width * height * 4) as usize];
        let dc = CreateCompatibleDC(null_mut());
        let read = GetDIBits(dc, info.hbmColor, 0, height as u32, buffer.as_mut_ptr().cast(), &mut header, DIB_RGB_COLORS);
        DeleteDC(dc);
        // An icon without an alpha channel would come out fully transparent; skip it.
        if read == height && buffer.chunks_exact(4).any(|px| px[3] != 0) {
            result = Some((width as u32, height as u32, buffer));
        }
    }
    if !info.hbmColor.is_null() {
        DeleteObject(info.hbmColor);
    }
    if !info.hbmMask.is_null() {
        DeleteObject(info.hbmMask);
    }
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_a_system_executables_icon_as_rgba() {
        let explorer = std::path::Path::new(r"C:\Windows\explorer.exe");
        let icon = super::icon(explorer, 64).expect("explorer.exe has an icon");
        assert!(icon.width >= 32 && icon.height >= 32);
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &icon.rgba).unwrap();
        assert_eq!(bytes.len() as u32, icon.width * icon.height * 4);
        assert!(super::icon(std::path::Path::new(r"C:\does-not-exist.exe"), 64).is_none());
    }
}
