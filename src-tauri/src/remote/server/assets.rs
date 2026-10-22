use super::super::RemoteCore;
use axum::extract::{Request, State};
use axum::http::header;
#[cfg(debug_assertions)]
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

pub(super) async fn asset(State(core): State<Arc<RemoteCore>>, req: Request) -> Response {
    let root = req.uri().path().trim_start_matches('/');
    let relative = if root.is_empty() { "remote.html" } else { root };
    let candidate = std::path::Path::new(relative);
    if candidate
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return not_found();
    }
    let resolved = candidate.to_string_lossy().replace('\\', "/");
    // In development the frontend is served live by Vite: `cargo` does not
    // rebuild when only `dist/` changes, so the embedded assets go stale during
    // `tauri dev`. Release builds keep serving the embedded assets below.
    #[cfg(debug_assertions)]
    {
        let target = match req.uri().query() {
            Some(query) => format!("/{resolved}?{query}"),
            None => format!("/{resolved}"),
        };
        if let Some(response) = dev_asset(&core, &target, req.headers()).await {
            return response;
        }
    }
    let resolver = core.app.asset_resolver();
    let asset = resolver.get(resolved.clone()).or_else(|| {
        if resolved.contains('.') {
            None
        } else {
            resolver.get(format!("{resolved}/remote.html"))
        }
    });
    let Some(asset) = asset else {
        return not_found();
    };
    let mut response = (
        [(header::CONTENT_TYPE, asset.mime_type.clone())],
        asset.bytes,
    )
        .into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    if let Some(csp) = asset.csp_header {
        if let Ok(value) = header::HeaderValue::from_str(&csp) {
            headers.insert(header::CONTENT_SECURITY_POLICY, value);
        }
    }
    response
}

/// Proxies an asset request to the Vite dev server named by the Tauri
/// `devUrl`, so the phone always gets the live frontend during `tauri dev`.
/// Returns `None` when Vite is unreachable, is not a dev build, or has no
/// matching asset, so the caller can fall back to the embedded assets.
#[cfg(debug_assertions)]
async fn dev_asset(core: &RemoteCore, target: &str, request: &HeaderMap) -> Option<Response> {
    let dev_url = core.app.config().build.dev_url.clone()?;
    let base = dev_url.as_str().trim_end_matches('/');
    fetch_dev_asset(base, target, request).await
}

// Vite picks the response format from these request headers: a `.css` import
// fetched as a module script must come back as JavaScript, while a stylesheet
// link gets plain CSS. Forward the browser's own values instead of guessing.
#[cfg(debug_assertions)]
const DEV_FORWARDED_HEADERS: [&str; 3] = ["accept", "sec-fetch-dest", "sec-fetch-mode"];

/// Fetches one asset from the Vite dev server and wraps it as a no-store
/// response. Split from [`dev_asset`] so it can be exercised against a running
/// `npm run dev` without a Tauri app.
#[cfg(debug_assertions)]
async fn fetch_dev_asset(base: &str, target: &str, request: &HeaderMap) -> Option<Response> {
    const DEV_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);
    let client = reqwest::Client::builder()
        .connect_timeout(DEV_TIMEOUT)
        .timeout(DEV_TIMEOUT)
        .build()
        .ok()?;
    let mut upstream = client.get(format!("{base}{target}"));
    for name in DEV_FORWARDED_HEADERS {
        if let Some(value) = request.get(name).and_then(|value| value.to_str().ok()) {
            upstream = upstream.header(name, value);
        }
    }
    let upstream = upstream.send().await.ok()?;
    if !upstream.status().is_success() {
        return None;
    }
    let status = StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::OK);
    let content_type = upstream
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    let bytes = upstream.bytes().await.ok()?.to_vec();
    let mut response = (status, [(header::CONTENT_TYPE, content_type)], bytes).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    Some(response)
}

fn not_found() -> Response {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "Swapper remote assets are missing. Run `npm run build` first, then restart Swapper.",
    )
        .into_response()
}

#[cfg(all(test, debug_assertions))]
mod dev_proxy_tests {
    use super::fetch_dev_asset;
    use axum::http::{HeaderMap, HeaderValue};

    fn module_request() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("accept", HeaderValue::from_static("*/*"));
        headers.insert("sec-fetch-dest", HeaderValue::from_static("script"));
        headers
    }

    /// Exercises the dev proxy against a live `npm run dev`. Ignored by default
    /// because it needs the Vite server; run it with
    /// `cargo test --manifest-path src-tauri/Cargo.toml dev_proxy -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "needs `npm run dev` on http://localhost:1420"]
    async fn remote_html_and_its_module_graph_come_from_vite() {
        let base =
            std::env::var("SWAPPER_DEV_URL").unwrap_or_else(|_| "http://localhost:1420".into());
        let html = fetch_dev_asset(&base, "/remote.html", &HeaderMap::new())
            .await
            .expect("Vite should serve remote.html");
        let body = axum::body::to_bytes(html.into_body(), 1 << 20)
            .await
            .unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(
            text.contains("<div id=\"root\">"),
            "unexpected HTML: {text}"
        );
        assert!(
            text.contains("/src/remote/main.tsx"),
            "no module entry: {text}"
        );

        let module = fetch_dev_asset(&base, "/src/remote/main.tsx", &module_request())
            .await
            .expect("Vite should serve the module graph");
        let module_body = axum::body::to_bytes(module.into_body(), 1 << 20)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&module_body).contains("createRoot"));
    }

    /// A CSS file imported from a module must come back as JavaScript, or the
    /// browser rejects the module and the phone page never mounts.
    #[tokio::test]
    #[ignore = "needs `npm run dev` on http://localhost:1420"]
    async fn css_imported_by_a_module_is_served_as_javascript() {
        let base =
            std::env::var("SWAPPER_DEV_URL").unwrap_or_else(|_| "http://localhost:1420".into());
        let css = fetch_dev_asset(&base, "/src/remote/remote.css", &module_request())
            .await
            .expect("Vite should serve the stylesheet module");
        let content_type = css
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(content_type.contains("javascript"), "got {content_type}");
    }
}
