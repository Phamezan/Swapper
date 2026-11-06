use url::Url;

pub(crate) fn validate_guide_source_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|_| "Invalid guide source URL.".to_string())?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || !matches!(
            url.host_str(),
            // Same hosts as HOSTS in the guides repo's scripts/validate.mjs; a guide citing any other host is dropped.
            Some("mobafire.com" | "www.mobafire.com" | "docs.google.com"
                | "youtube.com" | "www.youtube.com" | "youtu.be"
                | "reddit.com" | "www.reddit.com" | "old.reddit.com" | "lolmatchups.gg"
                | "www.inven.co.kr" | "talk.op.gg" | "mobalytics.gg" | "riven.saltyy.at")
        )
    {
        return Err("Guide sources must use HTTPS on an allowed host without credentials.".to_string());
    }
    Ok(url)
}

/// Opens a URL in the default browser. `explorer <url>` is not used: on some Windows
/// builds it shows an "open with" picker instead of handing the URL to the browser.
pub(crate) fn open_in_browser(url: &Url) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |text: &str| text.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (verb, file) = (wide("open"), wide(url.as_str()));
    let result = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) };
    // ShellExecuteW reports success as a value above 32.
    if result as isize > 32 { Ok(()) } else { Err(format!("Windows could not open the link (code {}).", result as isize)) }
}

#[tauri::command]
pub(crate) fn open_guide_source(url: String) -> Result<(), String> {
    // Launch the parsed URL, never the webview's raw string.
    open_in_browser(&validate_guide_source_url(&url)?).map_err(|error| format!("Could not open the guide source: {error}"))
}

#[cfg(test)]
mod tests {
    use super::validate_guide_source_url;

    #[test]
    fn guide_source_url_allowlist() {
        for raw in [
            "https://mobafire.com/league-of-legends/ahri-guide",
            "https://www.mobafire.com/league-of-legends/build/ahri-guide-123456",
            "https://docs.google.com/spreadsheets/d/example/edit#gid=0",
            "https://youtube.com/watch?v=example",
            "https://www.youtube.com/watch?v=example&t=20",
            "https://youtu.be/example",
            "https://www.reddit.com/r/summonerschool/comments/example",
            "https://old.reddit.com/r/example",
            "https://lolmatchups.gg/ahri/zed",
            "https://mobalytics.gg/lol/champions/ahri/counters",
            "https://www.inven.co.kr/board/lol/2778",
            "https://talk.op.gg/s/lol/free/1",
            "https://riven.saltyy.at/",
        ] {
            assert!(validate_guide_source_url(raw).is_ok(), "{raw}");
        }
        for raw in [
            "http://www.mobafire.com/guide",
            "https://example.com/guide",
            "https://mobafire.com.evil.io/guide",
            "https://evil-mobafire.com/guide",
            "https://evil.io/mobafire.com",
            "https://docs.google.com.evil.io/guide",
            "https://mobalytics.gg.evil.io/guide",
            "https://evil.reddit.com/r/example",
            "https://riven.saltyy.at.evil.io/",
            "file:///C:/Windows/System32/calc.exe",
            "javascript:alert(1)",
            "https://user:password@mobafire.com/guide",
            "https://user@www.youtube.com/watch",
            "https://:password@youtu.be/example",
            "https://mobafire.com@evil.io/guide",
            "not a URL",
        ] {
            assert!(validate_guide_source_url(raw).is_err(), "{raw}");
        }
        assert_eq!(
            validate_guide_source_url("https://www.mobafire.com/a b").unwrap().as_str(),
            "https://www.mobafire.com/a%20b"
        );
    }
}
