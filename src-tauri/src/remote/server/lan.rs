use super::super::{control, mdns, RemoteCore};
use super::actions::action_allowed;
use axum::extract::{Query, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct LanSession(pub(super) String);

/// Paths that may be reached without a paired-device cookie. Everything else on
/// the LAN router requires `swapper_lan_session`.
fn is_public_lan_path(path: &str) -> bool {
    matches!(path, "/pair" | "/handoff" | "/handoff/ping")
}

pub(super) async fn authorize_lan(
    State(core): State<Arc<RemoteCore>>,
    mut request: Request,
    next: Next,
) -> Response {
    let peer = request
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|info| info.0.ip());
    let allowed_peer = peer.is_some_and(|ip| {
        ip.is_loopback()
            || match ip {
                std::net::IpAddr::V4(address) => address.is_private(),
                std::net::IpAddr::V6(_) => false,
            }
    });
    if !allowed_peer {
        return StatusCode::NOT_FOUND.into_response();
    }
    if is_public_lan_path(request.uri().path()) {
        return next.run(request).await;
    }
    let session = request
        .headers()
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|cookie| {
                let (name, value) = cookie.trim().split_once('=')?;
                (name == "swapper_lan_session").then_some(value)
            })
        })
        .map(str::to_string);
    let Some(session) = session.filter(|session| core.has_lan_session(session)) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    request.extensions_mut().insert(LanSession(session));
    next.run(request).await
}

#[derive(Deserialize, Default)]
pub(super) struct PairingQuery {
    token: Option<String>,
}

pub(super) async fn pair_lan_device(
    State(core): State<Arc<RemoteCore>>,
    Query(query): Query<PairingQuery>,
    headers: HeaderMap,
) -> Response {
    let Some(token) = query.token else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let device_name = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(infer_lan_device_name)
        .unwrap_or("Phone");
    let session = match core.pair_lan_device(&token, device_name) {
        Ok(Some(session)) => session,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "LAN pairing is unavailable. Reset LAN Access in Swapper Settings and try again.",
            )
                .into_response();
        }
    };
    let mut response = axum::response::Redirect::to("/").into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        format!(
            "swapper_lan_session={session}; Max-Age=31536000; HttpOnly; SameSite=Strict; Path=/"
        )
        .parse()
        .expect("valid pairing cookie"),
    );
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        "no-referrer".parse().expect("valid referrer policy"),
    );
    response
}

/// Mints a one-time token for the calling paired device. The LAN middleware has
/// already authenticated the session and recorded which device it belongs to.
pub(super) async fn mint_handoff(
    State(core): State<Arc<RemoteCore>>,
    headers: HeaderMap,
    Extension(LanSession(session)): Extension<LanSession>,
) -> Response {
    if !action_allowed(&headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(control::ActionResult::error("Invalid action request.")),
        )
            .into_response();
    }
    match core.mint_lan_handoff(&session) {
        Ok(token) => (StatusCode::OK, Json(serde_json::json!({ "token": token }))).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(Deserialize, Default)]
pub(super) struct HandoffQuery {
    token: Option<String>,
}

const HANDOFF_CONTINUE_PAGE: &str = r#"<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta http-equiv="refresh" content="0;url=/"><title>Swapper</title></head><body><p><a href="/">Continue to Swapper</a></p></body></html>"#;

/// Sets the paired device's cookie on the `.local` origin after a one-time
/// token handoff. The `Host` must be the mDNS name so the cookie is scoped to
/// the stable hostname; anything invalid returns a plain 404 with no detail.
pub(super) async fn consume_handoff(
    State(core): State<Arc<RemoteCore>>,
    Query(query): Query<HandoffQuery>,
    headers: HeaderMap,
) -> Response {
    let host_is_local = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .is_some_and(mdns::is_local_host);
    let Some(token) = query.token.as_deref().filter(|token| !token.is_empty()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !host_is_local {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(credential) = core.consume_lan_handoff(token) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    // The phone arrives here from the IP origin, a different site, so a 302 to
    // "/" would still be a cross-site navigation and the browser would withhold
    // the SameSite=Strict cookie set just now. A page that moves on by itself
    // starts a same-site navigation, which carries the cookie.
    let mut response = (
        [(header::CACHE_CONTROL, "no-store")],
        axum::response::Html(HANDOFF_CONTINUE_PAGE),
    )
        .into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        format!(
            "swapper_lan_session={credential}; Max-Age=31536000; HttpOnly; SameSite=Strict; Path=/"
        )
        .parse()
        .expect("valid handoff cookie"),
    );
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        "no-referrer".parse().expect("valid referrer policy"),
    );
    response
}

/// Cheap reachability probe the IP page uses to decide whether offering the
/// `.local` handoff makes sense. Public on the LAN, no state, no body.
pub(super) async fn handoff_ping() -> StatusCode {
    StatusCode::NO_CONTENT
}

fn infer_lan_device_name(user_agent: &str) -> &'static str {
    let user_agent = user_agent.to_ascii_lowercase();
    if user_agent.contains("ipad") {
        "iPad"
    } else if user_agent.contains("iphone") {
        "iPhone"
    } else if user_agent.contains("android") && user_agent.contains("mobile") {
        "Android phone"
    } else if user_agent.contains("android") {
        "Android tablet"
    } else if user_agent.contains("windows") {
        "Windows PC"
    } else if (user_agent.contains("macintosh") || user_agent.contains("mac os x"))
        && user_agent.contains("mobile/")
    {
        // iPadOS Safari ships a desktop "Macintosh" UA and adds a Mobile/ token.
        "iPad"
    } else if user_agent.contains("macintosh") || user_agent.contains("mac os x") {
        "Mac"
    } else if user_agent.contains("linux") {
        "Linux PC"
    } else if user_agent.contains("mobile") {
        "Mobile browser"
    } else {
        "Phone"
    }
}

#[cfg(test)]
mod name_tests {
    use super::infer_lan_device_name;

    #[test]
    fn names_phones_and_computers_from_the_user_agent() {
        assert_eq!(
            infer_lan_device_name("Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)"),
            "iPhone"
        );
        assert_eq!(
            infer_lan_device_name("Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit Mobile"),
            "Android phone"
        );
        assert_eq!(
            infer_lan_device_name("Mozilla/5.0 (Linux; Android 13; SM-X700) AppleWebKit"),
            "Android tablet"
        );
        assert_eq!(
            infer_lan_device_name("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/126"),
            "Windows PC"
        );
        // iPadOS Safari masquerades as a Macintosh, but adds a Mobile/ token.
        assert_eq!(
            infer_lan_device_name(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
                 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1"
            ),
            "iPad"
        );
        assert_eq!(
            infer_lan_device_name(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
                 (KHTML, like Gecko) Version/17.0 Safari/605.1.15"
            ),
            "Mac"
        );
        assert_eq!(infer_lan_device_name("curl/8.0"), "Phone");
    }
}
