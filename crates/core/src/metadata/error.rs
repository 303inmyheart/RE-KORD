//! Machine-readable errors for the metadata / studio endpoints.
//!
//! Functions return `anyhow::Error`; when the cause is one of ours it wraps a
//! [`MetaError`] carrying a stable `code` (the client translates it) and the
//! HTTP status to use. [`classify`] turns any error into
//! `(status, code, message)`, mapping a few legacy strings too.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaError {
    /// Stable snake_case code, e.g. `album_not_found`.
    pub code: &'static str,
    /// HTTP status the handler should answer with.
    pub status: u16,
    /// Human message (English, for logs / fallback display).
    pub message: String,
}

impl fmt::Display for MetaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for MetaError {}

impl MetaError {
    pub fn new(code: &'static str, status: u16, message: impl Into<String>) -> Self {
        Self {
            code,
            status,
            message: message.into(),
        }
    }

    /// As an `anyhow::Error` (for `return Err(MetaError::x().into())`).
    pub fn err(self) -> anyhow::Error {
        anyhow::Error::new(self)
    }

    pub fn album_not_found() -> Self {
        Self::new("album_not_found", 404, "album not found")
    }
    pub fn artist_not_found() -> Self {
        Self::new("artist_not_found", 404, "artist not found")
    }
    pub fn track_not_found() -> Self {
        Self::new("track_not_found", 404, "track not found")
    }
    pub fn album_path_required() -> Self {
        Self::new("album_path_required", 400, "album path required")
    }
    pub fn artist_required() -> Self {
        Self::new("artist_required", 400, "artist required")
    }
    pub fn invalid_path() -> Self {
        Self::new("invalid_path", 400, "invalid path")
    }
    pub fn no_metadata_found() -> Self {
        Self::new("no_metadata_found", 404, "no metadata found")
    }
    pub fn no_match() -> Self {
        Self::new(
            "no_match",
            404,
            "no provider result matches this track closely enough",
        )
    }
    pub fn query_too_short() -> Self {
        Self::new("query_too_short", 400, "query too short")
    }
    pub fn url_host_not_allowed() -> Self {
        Self::new("url_host_not_allowed", 400, "URL host not allowed")
    }
    pub fn invalid_url() -> Self {
        Self::new("invalid_url", 400, "invalid URL")
    }
    pub fn image_invalid() -> Self {
        Self::new("image_invalid", 400, "not a valid image")
    }
    pub fn image_too_large() -> Self {
        Self::new("image_too_large", 413, "image too large")
    }
    pub fn image_fetch_failed(detail: impl fmt::Display) -> Self {
        Self::new(
            "image_fetch_failed",
            502,
            format!("image download failed: {detail}"),
        )
    }
    pub fn discogs_rate_limited() -> Self {
        Self::new("discogs_rate_limited", 429, "Discogs rate limit")
    }
    pub fn discogs_unauthorized() -> Self {
        Self::new(
            "discogs_unauthorized",
            401,
            "Discogs token missing or rejected",
        )
    }
    pub fn discogs_release_mismatch(score: f64) -> Self {
        Self::new(
            "discogs_release_mismatch",
            409,
            format!("Discogs release does not match album folder (score {score})"),
        )
    }
    pub fn invalid_release_id() -> Self {
        Self::new("invalid_release_id", 400, "invalid release id")
    }
    pub fn upstream_unavailable(detail: impl fmt::Display) -> Self {
        Self::new("upstream_unavailable", 502, detail.to_string())
    }
    pub fn upstream_timeout(detail: impl fmt::Display) -> Self {
        Self::new("upstream_timeout", 504, detail.to_string())
    }
    pub fn missing_artist_or_title() -> Self {
        Self::new("missing_artist_or_title", 400, "Missing artist or title")
    }
    pub fn invalid_request(detail: impl Into<String>) -> Self {
        Self::new("invalid_request", 400, detail)
    }
}

/// Every code this module can produce (documentation / client tables).
pub const ALL_CODES: &[&str] = &[
    "album_not_found",
    "artist_not_found",
    "track_not_found",
    "album_path_required",
    "artist_required",
    "invalid_path",
    "no_metadata_found",
    "no_match",
    "query_too_short",
    "url_host_not_allowed",
    "invalid_url",
    "image_invalid",
    "image_too_large",
    "image_fetch_failed",
    "discogs_rate_limited",
    "discogs_unauthorized",
    "discogs_release_mismatch",
    "invalid_release_id",
    "upstream_unavailable",
    "upstream_timeout",
    "missing_artist_or_title",
    "invalid_request",
    "internal_error",
];

/// A non-fatal failure of one source in a multi-source search (reported
/// next to the partial results instead of failing the whole request).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SourceError {
    /// `wikipedia`, `wikiquote`, `lastfm`, `discogs`, `theaudiodb`,
    /// `deezer`, `itunes`, `musicbrainz`, `coverart`.
    pub source: String,
    /// `upstream_timeout`, `upstream_unavailable`, `rate_limited`,
    /// `discogs_rate_limited`, `discogs_unauthorized`, `upstream_invalid`.
    pub code: String,
    pub message: String,
}

impl SourceError {
    pub fn new(source: &str, code: &str, message: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            code: code.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.source, self.message)
    }
}

impl std::error::Error for SourceError {}

/// Codes used in [`SourceError::code`].
pub const SOURCE_CODES: &[&str] = &[
    "upstream_timeout",
    "upstream_unavailable",
    "upstream_invalid",
    "rate_limited",
    "discogs_rate_limited",
    "discogs_unauthorized",
];

/// Map a network error to a [`MetaError`] (timeout → 504, else 502).
pub fn from_reqwest(source: &str, e: &reqwest::Error) -> MetaError {
    if e.is_timeout() {
        MetaError::upstream_timeout(format!("{source}: timeout"))
    } else {
        MetaError::upstream_unavailable(format!("{source}: {e}"))
    }
}

/// `(status, code, message)` for any error returned by the metadata,
/// artwork and entity-info functions.
pub fn classify(e: &anyhow::Error) -> (u16, &'static str, String) {
    for cause in e.chain() {
        if let Some(m) = cause.downcast_ref::<MetaError>() {
            return (m.status, m.code, m.message.clone());
        }
        if let Some(se) = cause.downcast_ref::<SourceError>() {
            let (status, code) = match se.code.as_str() {
                "discogs_rate_limited" => (429, "discogs_rate_limited"),
                "discogs_unauthorized" => (401, "discogs_unauthorized"),
                "upstream_timeout" => (504, "upstream_timeout"),
                _ => (502, "upstream_unavailable"),
            };
            return (status, code, se.to_string());
        }
        if let Some(r) = cause.downcast_ref::<reqwest::Error>() {
            let m = from_reqwest("upstream", r);
            return (m.status, m.code, e.to_string());
        }
    }
    let msg = e.to_string();
    let lower = msg.to_ascii_lowercase();
    let (status, code) = if lower.contains("album not found") {
        (404, "album_not_found")
    } else if lower.contains("artist not found") {
        (404, "artist_not_found")
    } else if lower.contains("track not found") {
        (404, "track_not_found")
    } else if lower.contains("invalid relative path")
        || lower.contains("invalid path")
        || lower.contains("invalid directory name")
    {
        (400, "invalid_path")
    } else if lower.contains("rate limit") {
        (429, "discogs_rate_limited")
    } else if lower.contains("no metadata found") {
        (404, "no_metadata_found")
    } else {
        (500, "internal_error")
    };
    (status, code, msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_finds_wrapped_meta_errors() {
        let e = MetaError::query_too_short().err().context("artwork search");
        let (status, code, _) = classify(&e);
        assert_eq!((status, code), (400, "query_too_short"));
        let (status, code, _) = classify(&anyhow::anyhow!("album not found"));
        assert_eq!((status, code), (404, "album_not_found"));
        for c in ALL_CODES {
            assert!(c.chars().all(|x| x.is_ascii_lowercase() || x == '_'));
        }
    }
}
