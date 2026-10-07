//! Outgoing HTTP for the podcasts module: public hosts only (the SSRF guard of
//! `metadata::artwork`, checked again on every redirect hop and pinned against
//! DNS rebinding), a sane User-Agent, timeouts and size caps.

use super::PodcastError;
use crate::metadata::artwork::{host_name_blocked, ip_blocked};
use futures::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue};
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

/// Redirect hops followed by hand (each one is checked again).
pub const MAX_REDIRECTS: usize = 5;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub fn user_agent() -> String {
    format!(
        "RE-KORD/{} (podcasts; +https://re-kord.com)",
        env!("CARGO_PKG_VERSION")
    )
}

/// Which addresses outgoing requests may reach. Production: public ones only.
/// Tests add loopback addresses of their local fixtures; nothing outside Rust
/// code (no setting, no environment variable) can widen it.
#[derive(Debug, Clone, Default)]
pub struct NetPolicy {
    pub extra_allowed: Vec<IpAddr>,
}

impl NetPolicy {
    fn allows(&self, ip: IpAddr) -> bool {
        !ip_blocked(ip) || self.extra_allowed.contains(&ip)
    }
}

/// http(s), no credentials in the URL.
pub fn parse_public_url(raw: &str) -> Result<url::Url, PodcastError> {
    let raw = raw.trim();
    let raw = if let Some(rest) = raw.strip_prefix("//") {
        format!("https://{rest}")
    } else {
        raw.to_string()
    };
    let url = url::Url::parse(&raw).map_err(|_| PodcastError::InvalidUrl)?;
    check_url_shape(&url)?;
    Ok(url)
}

fn check_url_shape(url: &url::Url) -> Result<(), PodcastError> {
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(PodcastError::InvalidUrl);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(PodcastError::InvalidUrl);
    }
    if url.host().is_none() {
        return Err(PodcastError::InvalidUrl);
    }
    Ok(())
}

/// Resolve `url`'s host to one allowed address to pin the connection to.
pub async fn resolve_allowed(
    url: &url::Url,
    policy: &NetPolicy,
) -> Result<SocketAddr, PodcastError> {
    resolve_allowed_all(url, policy).await.map(|addrs| addrs[0])
}

/// Every address of `url`'s host, all allowed, IPv4 first (hosts with a
/// broken IPv6 route are common; the client tries them in order).
pub async fn resolve_allowed_all(
    url: &url::Url,
    policy: &NetPolicy,
) -> Result<Vec<SocketAddr>, PodcastError> {
    let port = url
        .port_or_known_default()
        .ok_or(PodcastError::InvalidUrl)?;
    let host = match url.host() {
        Some(url::Host::Ipv4(ip)) => {
            let ip = IpAddr::V4(ip);
            return if policy.allows(ip) {
                Ok(vec![SocketAddr::new(ip, port)])
            } else {
                Err(PodcastError::UrlNotAllowed)
            };
        }
        Some(url::Host::Ipv6(ip)) => {
            let ip = IpAddr::V6(ip);
            return if policy.allows(ip) {
                Ok(vec![SocketAddr::new(ip, port)])
            } else {
                Err(PodcastError::UrlNotAllowed)
            };
        }
        Some(url::Host::Domain(d)) => d.to_string(),
        None => return Err(PodcastError::InvalidUrl),
    };
    if host_name_blocked(&host) {
        return Err(PodcastError::UrlNotAllowed);
    }
    let mut addrs: Vec<SocketAddr> = tokio::time::timeout(
        CONNECT_TIMEOUT,
        tokio::net::lookup_host((host.as_str(), port)),
    )
    .await
    .map_err(|_| PodcastError::Timeout)?
    .map_err(|e| PodcastError::Fetch(format!("resolve {host}: {e}")))?
    .collect();
    if addrs.is_empty() {
        return Err(PodcastError::Fetch(format!("{host} did not resolve")));
    }
    // One internal answer poisons the name (split horizon / rebinding).
    if addrs.iter().any(|a| !policy.allows(a.ip())) {
        return Err(PodcastError::UrlNotAllowed);
    }
    addrs.sort_by_key(|a| a.is_ipv6());
    addrs.dedup();
    Ok(addrs)
}

/// One request, redirects followed by hand: every hop is a public http(s) URL.
pub struct Request<'a> {
    pub url: url::Url,
    pub headers: HeaderMap,
    /// Whole-request timeout (metadata); `None` for streams (they enforce an
    /// idle timeout while reading instead).
    pub timeout: Option<Duration>,
    pub policy: &'a NetPolicy,
}

pub struct Opened {
    pub final_url: url::Url,
    pub response: reqwest::Response,
}

pub async fn open(req: Request<'_>) -> Result<Opened, PodcastError> {
    let mut url = req.url.clone();
    for _ in 0..=MAX_REDIRECTS {
        check_url_shape(&url)?;
        let addrs = resolve_allowed_all(&url, req.policy).await?;
        let mut builder = reqwest::Client::builder()
            .user_agent(user_agent())
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none());
        if let Some(t) = req.timeout {
            builder = builder.timeout(t);
        }
        if let Some(domain) = url.domain() {
            builder = builder.resolve_to_addrs(domain, &addrs);
        }
        let client = builder
            .build()
            .map_err(|e| PodcastError::Fetch(e.to_string()))?;
        let res = client
            .get(url.clone())
            .headers(req.headers.clone())
            .send()
            .await
            .map_err(classify_reqwest)?;
        if res.status().is_redirection() {
            let Some(loc) = res
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
            else {
                return Err(PodcastError::Fetch("redirect without location".into()));
            };
            url = url
                .join(loc.trim())
                .map_err(|_| PodcastError::Fetch("bad redirect location".into()))?;
            continue;
        }
        return Ok(Opened {
            final_url: url,
            response: res,
        });
    }
    Err(PodcastError::Fetch("too many redirects".into()))
}

pub fn classify_reqwest(e: reqwest::Error) -> PodcastError {
    if e.is_timeout() {
        PodcastError::Timeout
    } else {
        PodcastError::Fetch(e.to_string())
    }
}

/// Validators of a previous answer, sent as `If-None-Match` / `If-Modified-Since`.
#[derive(Debug, Clone, Default)]
pub struct Validators {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

#[allow(clippy::large_enum_variant)] // short-lived, moved once
pub enum Fetched {
    NotModified,
    Body {
        final_url: url::Url,
        content_type: String,
        body: Vec<u8>,
        validators: Validators,
        /// Station name an Icecast / Shoutcast server announces.
        icy_name: Option<String>,
    },
}

/// GET a document (feed, page, playlist, JSON) up to `max_bytes`.
pub async fn fetch_document(
    url: &url::Url,
    policy: &NetPolicy,
    accept: &str,
    max_bytes: usize,
    validators: Option<&Validators>,
) -> Result<Fetched, PodcastError> {
    let mut headers = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(accept) {
        headers.insert(reqwest::header::ACCEPT, v);
    }
    if let Some(v) = validators {
        if let Some(etag) = v
            .etag
            .as_deref()
            .and_then(|s| HeaderValue::from_str(s).ok())
        {
            headers.insert(reqwest::header::IF_NONE_MATCH, etag);
        }
        if let Some(lm) = v
            .last_modified
            .as_deref()
            .and_then(|s| HeaderValue::from_str(s).ok())
        {
            headers.insert(reqwest::header::IF_MODIFIED_SINCE, lm);
        }
    }
    let opened = open(Request {
        url: url.clone(),
        headers,
        timeout: Some(Duration::from_secs(20)),
        policy,
    })
    .await?;
    let res = opened.response;
    if res.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(Fetched::NotModified);
    }
    if !res.status().is_success() {
        return Err(PodcastError::Http(res.status().as_u16()));
    }
    let header = |name: reqwest::header::HeaderName| {
        res.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let content_type = header(reqwest::header::CONTENT_TYPE)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let validators = Validators {
        etag: header(reqwest::header::ETAG),
        last_modified: header(reqwest::header::LAST_MODIFIED),
    };
    let icy_name = res
        .headers()
        .get("icy-name")
        .map(|v| String::from_utf8_lossy(v.as_bytes()).trim().to_string())
        .filter(|s| !s.is_empty());
    let body = read_capped(res, max_bytes, &content_type).await?;
    Ok(Fetched::Body {
        final_url: opened.final_url,
        content_type,
        body,
        validators,
        icy_name,
    })
}

/// Read a body up to `max_bytes`. Audio is never read in full (a live stream
/// would never end): only a small head comes back for sniffing.
async fn read_capped(
    res: reqwest::Response,
    max_bytes: usize,
    content_type: &str,
) -> Result<Vec<u8>, PodcastError> {
    let audio = is_audio_type(content_type);
    let cap = if audio { 4096 } else { max_bytes };
    if !audio && res.content_length().is_some_and(|n| n > max_bytes as u64) {
        return Err(PodcastError::TooLarge);
    }
    let mut body = Vec::new();
    let mut stream = res.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(classify_reqwest)?;
        if body.len() + chunk.len() > cap {
            if audio {
                let take = cap - body.len();
                body.extend_from_slice(&chunk[..take]);
                return Ok(body);
            }
            return Err(PodcastError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

pub fn is_audio_type(content_type: &str) -> bool {
    let ct = content_type.split(';').next().unwrap_or("").trim();
    (ct.starts_with("audio/") && !is_playlist_type(ct))
        || ct == "application/ogg"
        || ct == "video/mp4"
        || ct == "application/octet-stream+audio"
}

pub fn is_playlist_type(content_type: &str) -> bool {
    let ct = content_type.split(';').next().unwrap_or("").trim();
    matches!(
        ct,
        "audio/x-mpegurl"
            | "audio/mpegurl"
            | "application/x-mpegurl"
            | "application/vnd.apple.mpegurl"
            | "audio/x-scpls"
            | "application/pls+xml"
            | "audio/scpls"
    )
}

/// GET JSON (RTL API) with the same rules.
pub async fn fetch_json(
    url: &url::Url,
    policy: &NetPolicy,
    max_bytes: usize,
) -> Result<serde_json::Value, PodcastError> {
    match fetch_document(url, policy, "application/json", max_bytes, None).await? {
        Fetched::Body { body, .. } => {
            serde_json::from_slice(&body).map_err(|e| PodcastError::Parse(e.to_string()))
        }
        Fetched::NotModified => Err(PodcastError::Parse("unexpected 304".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_http_and_credentials() {
        assert!(parse_public_url("ftp://example.com/a").is_err());
        assert!(parse_public_url("https://user:pw@example.com/a").is_err());
        assert!(parse_public_url("not a url").is_err());
        assert_eq!(
            parse_public_url("//example.com/feed").unwrap().as_str(),
            "https://example.com/feed"
        );
    }

    #[tokio::test]
    async fn private_and_local_hosts_are_refused() {
        let policy = NetPolicy::default();
        for raw in [
            "http://127.0.0.1/feed",
            "http://10.0.0.5/feed",
            "http://192.168.1.10/feed",
            "http://[::1]/feed",
            "http://localhost/feed",
            "http://printer.local/feed",
            "http://169.254.169.254/latest/meta-data",
        ] {
            let url = parse_public_url(raw).unwrap();
            assert!(
                matches!(
                    resolve_allowed(&url, &policy).await,
                    Err(PodcastError::UrlNotAllowed)
                ),
                "{raw} must be refused"
            );
        }
        let allowed = NetPolicy {
            extra_allowed: vec!["127.0.0.1".parse().unwrap()],
        };
        let url = parse_public_url("http://127.0.0.1:9/feed").unwrap();
        assert!(resolve_allowed(&url, &allowed).await.is_ok());
        let other = parse_public_url("http://127.0.0.2:9/feed").unwrap();
        assert!(resolve_allowed(&other, &allowed).await.is_err());
    }

    #[test]
    fn audio_and_playlist_types() {
        assert!(is_audio_type("audio/mpeg"));
        assert!(is_audio_type("audio/aacp; charset=x"));
        assert!(!is_audio_type("audio/x-mpegurl"));
        assert!(is_playlist_type("audio/x-scpls"));
        assert!(!is_audio_type("text/html"));
    }
}
