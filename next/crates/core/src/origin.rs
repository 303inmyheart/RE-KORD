//! Browser origin policy (parity legacy `createApp.mjs` CORS guard).
//!
//! The hub listens on the LAN and on loopback, so any web page open on the hub
//! machine could otherwise script it. Allowed browser origins are:
//! - same origin (Origin host == `Host` / `X-Forwarded-Host`), i.e. the
//!   hub-served SPA, also through the Cloudflare tunnel;
//! - the Tauri shells (`tauri://localhost`, `http(s)://tauri.localhost`);
//! - the Vite dev servers (`localhost` / `127.0.0.1` on 7421 / 7422);
//! - `REKORD_ALLOWED_ORIGINS` (comma separated).
//!
//! Foreign origins get 403 `cross_origin_forbidden` on mutating methods and no
//! `Access-Control-Allow-Origin` on reads; they never count as local (see
//! `perm::is_local_request`).

use axum::body::Body;
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::time::Duration;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};

const BUILTIN_ORIGINS: &[&str] = &[
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
    "http://127.0.0.1:7422",
    "http://localhost:7422",
    "http://127.0.0.1:7421",
    "http://localhost:7421",
];

pub const ALLOWED_ORIGINS_ENV: &str = "REKORD_ALLOWED_ORIGINS";

fn normalize_origin(raw: &str) -> String {
    raw.trim().trim_end_matches('/').to_ascii_lowercase()
}

fn env_origins() -> Vec<String> {
    std::env::var(ALLOWED_ORIGINS_ENV)
        .map(|v| {
            v.split(',')
                .map(normalize_origin)
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// `host[:port]` of an origin, with the scheme's default port dropped.
fn origin_authority(origin: &str) -> Option<String> {
    let url = url::Url::parse(origin).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    Some(match url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host,
    })
}

fn strip_default_port(authority: &str) -> String {
    let a = authority.trim().to_ascii_lowercase();
    for suffix in [":80", ":443"] {
        if let Some(bare) = a.strip_suffix(suffix) {
            return bare.to_string();
        }
    }
    a
}

/// True when `origin` may drive this hub given the request headers.
pub fn origin_allowed(origin: &str, headers: &HeaderMap) -> bool {
    let norm = normalize_origin(origin);
    if norm.is_empty() || norm == "null" {
        return false;
    }
    if BUILTIN_ORIGINS.contains(&norm.as_str()) || env_origins().contains(&norm) {
        return true;
    }
    let Some(authority) = origin_authority(&norm) else {
        return false;
    };
    [header::HOST.as_str(), "x-forwarded-host"]
        .iter()
        .filter_map(|h| headers.get(*h).and_then(|v| v.to_str().ok()))
        .flat_map(|v| v.split(','))
        .any(|host| strip_default_port(host) == authority)
}

/// Browser context of a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginKind {
    /// No browser cross-site signal (CLI, native apps, top-level navigation).
    None,
    /// Allowed origin (same-origin SPA, Tauri, dev).
    Allowed,
    /// Any other web page.
    Foreign,
}

pub fn classify(headers: &HeaderMap) -> OriginKind {
    if let Some(origin) = headers.get(header::ORIGIN) {
        let Ok(origin) = origin.to_str() else {
            return OriginKind::Foreign;
        };
        return if origin_allowed(origin, headers) {
            OriginKind::Allowed
        } else {
            OriginKind::Foreign
        };
    }
    // No Origin: no-cors subresources (`<img>`, `<audio>`) and plain GET
    // navigations. Fetch metadata still tells a cross-site embed apart.
    match headers
        .get("sec-fetch-site")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("cross-site") | Some("same-site") => OriginKind::Foreign,
        _ => OriginKind::None,
    }
}

/// Requests a foreign page may still trigger (reads without ACAO, preflights).
fn is_safe_method(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

/// Reject mutating requests coming from foreign origins (runs before CORS).
pub async fn origin_guard(req: Request<Body>, next: Next) -> Response {
    if !is_safe_method(req.method())
        && req.headers().contains_key(header::ORIGIN)
        && classify(req.headers()) == OriginKind::Foreign
    {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "ok": false, "data": null, "error": "cross_origin_forbidden" })),
        )
            .into_response();
    }
    next.run(req).await
}

/// CORS for allowed origins only (credentials are not used: auth is by header).
pub fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin, parts| {
            origin
                .to_str()
                .map(|o| origin_allowed(o, &parts.headers))
                .unwrap_or(false)
        }))
        .allow_methods(AllowMethods::list([
            Method::GET,
            Method::HEAD,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ]))
        .allow_headers(AllowHeaders::mirror_request())
        .expose_headers([
            HeaderName::from_static("x-request-id"),
            header::CONTENT_DISPOSITION,
            header::CONTENT_RANGE,
            header::CONTENT_LENGTH,
            header::ACCEPT_RANGES,
            header::ETAG,
        ])
        .max_age(Duration::from_secs(600))
}

/// Baseline hardening headers on every response.
pub async fn security_headers(req: Request<Body>, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.entry(HeaderName::from_static("x-content-type-options"))
        .or_insert(HeaderValue::from_static("nosniff"));
    h.entry(HeaderName::from_static("x-frame-options"))
        .or_insert(HeaderValue::from_static("DENY"));
    h.entry(HeaderName::from_static("referrer-policy"))
        .or_insert(HeaderValue::from_static("no-referrer"));
    h.entry(HeaderName::from_static("permissions-policy"))
        .or_insert(HeaderValue::from_static(
            "camera=(self), microphone=(), geolocation=()",
        ));
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(
                HeaderName::from_bytes(k.as_bytes()).unwrap(),
                HeaderValue::from_str(v).unwrap(),
            );
        }
        h
    }

    #[test]
    fn same_origin_and_tauri_are_allowed() {
        let h = headers(&[("host", "192.168.1.5:7420")]);
        assert!(origin_allowed("http://192.168.1.5:7420", &h));
        assert!(origin_allowed("tauri://localhost", &h));
        assert!(origin_allowed("http://tauri.localhost", &h));
        assert!(origin_allowed("http://localhost:7422", &h));
        assert!(!origin_allowed("http://localhost:3000", &h));
        assert!(!origin_allowed("https://evil.example", &h));
        assert!(!origin_allowed("null", &h));
    }

    #[test]
    fn tunnel_same_origin_drops_default_port() {
        let h = headers(&[("host", "abc.trycloudflare.com")]);
        assert!(origin_allowed("https://abc.trycloudflare.com", &h));
    }

    #[test]
    fn cross_site_fetch_metadata_is_foreign() {
        let h = headers(&[("sec-fetch-site", "cross-site")]);
        assert_eq!(classify(&h), OriginKind::Foreign);
        let h = headers(&[("sec-fetch-site", "none")]);
        assert_eq!(classify(&h), OriginKind::None);
    }
}
