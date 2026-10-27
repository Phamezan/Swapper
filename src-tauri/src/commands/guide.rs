use std::process::Command;
use url::Url;

pub(crate) fn validate_guide_source_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|_| "Invalid guide source URL.".to_string())?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || !matches!(
            url.host_str(),
            Some("mobafire.com" | "www.mobafire.com" | "docs.google.com"
                | "youtube.com" | "www.youtube.com" | "youtu.be")
        )
    {
        return Err("Guide sources must use HTTPS on an allowed host without credentials.".to_string());
    }
    Ok(url)
}

#[tauri::command]
pub(crate) fn open_guide_source(url: String) -> Result<(), String> {
    let url = validate_guide_source_url(&url)?;
    // Launch the parsed URL, never the webview's raw string.
    Command::new("explorer")
        .arg(url.as_str())
        .spawn()
        .map(drop)
        .map_err(|error| format!("Could not open the guide source: {error}"))
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
