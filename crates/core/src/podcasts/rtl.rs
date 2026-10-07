//! play.rtl.it (RTL 102.5, Radio Zeta, Radio Freccia) programme archives.
//!
//! The pages publish no RSS feed and yt-dlp does not know the site, but the
//! page itself is filled from a public JSON API:
//!
//! - list: `cloud.rtl.it/api-play.rtl.it/web-app/1.0/archivio/<b>/podcast/info/<slug>/<count>/<offset>/`
//! - episode: the item's `detailUri` with `/web-app/` → `/media/` plus `0/`,
//!   whose `mediaInfo.uri` is a plain, Range-capable `.m4a`.

use super::feed::parse_date;
use super::net::{fetch_json, NetPolicy};
use super::{episode_key, Episode, PodcastError};
use serde_json::Value;

const API_BASE: &str = "https://cloud.rtl.it/api-play.rtl.it";
/// JSON answers are small (a few KB for a handful of items).
const MAX_JSON_BYTES: usize = 2 * 1024 * 1024;

/// `(broadcaster, slug)` of a play.rtl.it programme archive URL.
pub fn match_url(url: &url::Url) -> Option<(String, String)> {
    let host = url.host_str()?.to_ascii_lowercase();
    if host != "play.rtl.it" {
        return None;
    }
    let segs: Vec<&str> = url.path().split('/').filter(|s| !s.is_empty()).collect();
    let valid = |s: &str| {
        !s.is_empty()
            && s.len() <= 120
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    };
    match segs.as_slice() {
        // /archivio/1/podcast/info/giornale-orario/
        ["archivio", b, "podcast", "info", slug, ..] if valid(b) && valid(slug) => {
            Some((b.to_string(), slug.to_string()))
        }
        // /podcast/1/giornale-orario/ (programme) or an episode page under it
        ["podcast", b, slug, ..] if valid(b) && valid(slug) => {
            Some((b.to_string(), slug.to_string()))
        }
        _ => None,
    }
}

/// The public RSS feed RTL also publishes for a programme. Its dates carry a
/// wrong offset (UTC times labelled +0200), so it is only the fallback for
/// when the JSON API changes or fails.
pub fn rss_fallback_url(slug: &str) -> String {
    format!("https://rss.rtl.it/podcast/{slug}/?mediaType=201")
}

pub fn list_api_url(broadcaster: &str, slug: &str, count: u32) -> String {
    format!("{API_BASE}/web-app/1.0/archivio/{broadcaster}/podcast/info/{slug}/{count}/0/")
}

/// Media API of an episode from its `detailUri`.
pub fn media_api_url(detail_uri: &str) -> Option<String> {
    let u = url::Url::parse(detail_uri).ok()?;
    if u.host_str()? != "cloud.rtl.it" || !u.path().starts_with("/api-play.rtl.it/web-app/") {
        return None;
    }
    let path = u
        .path()
        .replacen("/api-play.rtl.it/web-app/", "/api-play.rtl.it/media/", 1);
    let path = if path.ends_with('/') {
        path
    } else {
        format!("{path}/")
    };
    Some(format!("https://cloud.rtl.it{path}0/"))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Listing {
    pub title: Option<String>,
    /// Episodes without `media_url` yet (resolved through the media API).
    pub episodes: Vec<(Episode, String)>,
}

fn thumb(item: &Value) -> Option<String> {
    let thumbs = item.get("thumbs")?;
    ["600", "400", "300", "1000"]
        .iter()
        .find_map(|k| thumbs.get(*k).and_then(Value::as_str))
        .map(str::to_string)
}

/// Programme title and items of a list answer (newest first, at most `limit`).
pub fn parse_listing(json: &Value, limit: usize) -> Result<Listing, PodcastError> {
    if json.get("success").and_then(Value::as_bool) == Some(false) {
        return Err(PodcastError::Parse("rtl api: success=false".into()));
    }
    let sections = json
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| PodcastError::Parse("rtl api: no data".into()))?;
    let mut title = None;
    let mut episodes = Vec::new();
    for sec in sections {
        if title.is_none() {
            title = sec
                .get("title")
                .and_then(Value::as_str)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }
        for item in sec
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(id) = item.get("id").and_then(Value::as_i64) else {
                continue;
            };
            let Some(detail) = item.get("detailUri").and_then(Value::as_str) else {
                continue;
            };
            let Some(media_api) = media_api_url(detail) else {
                continue;
            };
            let name = item
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            let published_at = item
                .get("date")
                .and_then(Value::as_str)
                .and_then(parse_date)
                .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
            episodes.push((
                Episode {
                    key: episode_key(&format!("rtl:{id}")),
                    title: if name.is_empty() {
                        format!("#{id}")
                    } else {
                        name
                    },
                    published_at,
                    duration_secs: None,
                    media_url: None,
                    page_url: None,
                    artwork_url: thumb(item),
                    mime: None,
                },
                media_api,
            ));
            if episodes.len() >= limit {
                return Ok(Listing { title, episodes });
            }
        }
    }
    Ok(Listing { title, episodes })
}

/// `(stream URL, duration)` out of a media API answer.
pub fn parse_media(json: &Value) -> Option<(String, Option<u32>)> {
    let info = json.pointer("/data/mediaInfo")?;
    let uri = info.get("uri").and_then(Value::as_str)?.trim();
    let u = url::Url::parse(uri).ok()?;
    if u.scheme() != "https" && u.scheme() != "http" {
        return None;
    }
    let duration = info
        .pointer("/nielsenDCRMetadata/length")
        .and_then(|v| {
            v.as_str()
                .map(str::to_string)
                .or_else(|| v.as_u64().map(|n| n.to_string()))
        })
        .and_then(|s| s.trim().parse::<u32>().ok())
        .filter(|n| *n > 0)
        .or_else(|| {
            info.get("duration")
                .and_then(Value::as_str)
                .and_then(super::feed::parse_duration)
        });
    Some((u.to_string(), duration))
}

/// Latest `limit` episodes of a programme, media URLs resolved (a few small
/// JSON requests, at most `limit` + 1).
pub async fn fetch_latest(
    broadcaster: &str,
    slug: &str,
    limit: u32,
    policy: &NetPolicy,
) -> Result<(Option<String>, Vec<Episode>), PodcastError> {
    let list_url = url::Url::parse(&list_api_url(broadcaster, slug, limit))
        .map_err(|_| PodcastError::InvalidUrl)?;
    let json = fetch_json(&list_url, policy, MAX_JSON_BYTES).await?;
    let listing = parse_listing(&json, limit as usize)?;
    let resolved = futures::future::join_all(listing.episodes.into_iter().map(
        |(mut ep, media_api)| async move {
            let url = url::Url::parse(&media_api).ok()?;
            let json = fetch_json(&url, policy, MAX_JSON_BYTES).await.ok()?;
            let (uri, duration) = parse_media(&json)?;
            ep.media_url = Some(uri);
            ep.duration_secs = duration;
            ep.mime = Some("audio/mp4".into());
            Some(ep)
        },
    ))
    .await;
    let episodes: Vec<Episode> = resolved.into_iter().flatten().collect();
    if episodes.is_empty() {
        return Err(PodcastError::NoEpisodes);
    }
    Ok((listing.title, episodes))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = include_str!("../../tests/fixtures/podcasts/rtl_list.json");
    const MEDIA: &str = include_str!("../../tests/fixtures/podcasts/rtl_media.json");

    #[test]
    fn matches_programme_urls() {
        let u = url::Url::parse("https://play.rtl.it/archivio/1/podcast/info/giornale-orario/")
            .unwrap();
        assert_eq!(match_url(&u), Some(("1".into(), "giornale-orario".into())));
        let ep = url::Url::parse(
            "https://play.rtl.it/podcast/1/giornale-orario/edizione-delle-15/174102/",
        )
        .unwrap();
        assert_eq!(match_url(&ep), Some(("1".into(), "giornale-orario".into())));
        let other = url::Url::parse("https://www.rtl.it/archivio/1/podcast/info/x/").unwrap();
        assert_eq!(match_url(&other), None);
        assert_eq!(
            list_api_url("1", "giornale-orario", 3),
            "https://cloud.rtl.it/api-play.rtl.it/web-app/1.0/archivio/1/podcast/info/giornale-orario/3/0/"
        );
    }

    #[test]
    fn parses_recorded_listing() {
        let json: Value = serde_json::from_str(LIST).unwrap();
        let listing = parse_listing(&json, 3).unwrap();
        assert_eq!(
            listing.title.as_deref(),
            Some("Giornale Orario di RTL 102.5")
        );
        assert_eq!(listing.episodes.len(), 3);
        let (first, media_api) = &listing.episodes[0];
        assert_eq!(first.title, "Edizione delle 15");
        assert_eq!(first.published_at.as_deref(), Some("2026-10-07T13:03:58Z"));
        assert!(first
            .artwork_url
            .as_deref()
            .unwrap()
            .contains("pictureprofile=600"));
        assert_eq!(
            media_api,
            "https://cloud.rtl.it/api-play.rtl.it/media/1.0/podcast/1/giornale-orario/edizione-delle-15/174102/0/"
        );
        // Stable keys: the same item always maps to the same episode key.
        assert_eq!(first.key, episode_key("rtl:174102"));
        assert_eq!(parse_listing(&json, 2).unwrap().episodes.len(), 2);
    }

    #[test]
    fn parses_recorded_media() {
        let json: Value = serde_json::from_str(MEDIA).unwrap();
        let (uri, duration) = parse_media(&json).unwrap();
        assert!(uri.ends_with("/vod/tSgn1XgXkaYG/aac_128_48000_2.m4a"));
        assert_eq!(duration, Some(226));
    }

    #[test]
    fn page_has_no_feed_to_discover() {
        let html = include_str!("../../tests/fixtures/podcasts/rtl_page_head.html");
        let base = url::Url::parse("https://play.rtl.it/archivio/1/podcast/info/giornale-orario/")
            .unwrap();
        let (links, title) = super::super::discover::discover_feed_links(html, &base);
        assert!(links.is_empty(), "{links:?}");
        assert!(title.is_some());
    }
}
