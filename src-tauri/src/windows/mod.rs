//! Windows helpers that are not Riot-specific.
//!
//! Riot policy (which images to stop, when a switch is allowed to proceed)
//! lives with its callers — see `crate::riot`.

pub mod process;

/// The file version from an executable's Windows version resource, e.g.
/// "1.18.0". A zero trailing build number is trimmed, which also keeps the
/// version out of the four-part number shape Doctor's sanitizer redacts.
/// `None` when the file has no readable version info.
#[cfg(windows)]
pub fn file_version(path: &std::path::Path) -> Option<String> {
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
    };

    /// Null-terminated UTF-16, the string form the version APIs take.
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let path = wide(&path.as_os_str().to_string_lossy());
    let size = unsafe { GetFileVersionInfoSizeW(path.as_ptr(), std::ptr::null_mut()) };
    if size == 0 {
        return None;
    }
    let mut data = vec![0u8; size as usize];
    let filled = unsafe {
        GetFileVersionInfoW(path.as_ptr(), 0, size, data.as_mut_ptr().cast())
    };
    if filled == 0 {
        return None;
    }
    let mut info: *mut VS_FIXEDFILEINFO = std::ptr::null_mut();
    let mut length = 0u32;
    let found = unsafe {
        VerQueryValueW(
            data.as_ptr().cast(),
            wide("\\").as_ptr(),
            &mut info as *mut *mut VS_FIXEDFILEINFO as *mut _,
            &mut length,
        )
    };
    if found == 0 || info.is_null() {
        return None;
    }
    let info = unsafe { &*info };
    if info.dwSignature != 0xFEEF_04BD {
        return None;
    }
    let build = info.dwFileVersionLS & 0xFFFF;
    if build == 0 {
        Some(format!(
            "{}.{}.{}",
            info.dwFileVersionMS >> 16,
            info.dwFileVersionMS & 0xFFFF,
            info.dwFileVersionLS >> 16
        ))
    } else {
        Some(format!(
            "{}.{}.{}.{}",
            info.dwFileVersionMS >> 16,
            info.dwFileVersionMS & 0xFFFF,
            info.dwFileVersionLS >> 16,
            build
        ))
    }
}

#[cfg(not(windows))]
pub fn file_version(_path: &std::path::Path) -> Option<String> {
    None
}
