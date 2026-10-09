//! RSS 2.0 / RSS 1.0 / Atom parsing, tolerant of what podcast hosts ship:
//! iTunes and Media RSS tags, durations as seconds or `HH:MM:SS`, missing
//! durations, relative enclosure URLs, RFC 2822 dates with named zones.

use super::xml::{clean_text, decode_text, Token, Tokenizer};
use super::{episode_key, Episode};
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Feed {
    pub title: Option<String>,
    pub artwork_url: Option<String>,
    pub episodes: Vec<Episode>,
}

/// Is this body an RSS / Atom document? (Cheap sniff of the first tags.)
pub fn looks_like_feed(body: &[u8]) -> bool {
    let head_len = body.len().min(4096);
    let head = String::from_utf8_lossy(&body[..head_len]).to_ascii_lowercase();
    head.contains("<rss") || head.contains("<feed") || head.contains("<rdf:rdf")
}

#[derive(Debug, Default)]
struct Node {
    name: String,
    attrs: Vec<(String, String)>,
    text: String,
    children: Vec<Node>,
}

impl Node {
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.trim())
            .filter(|v| !v.is_empty())
    }

    fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    fn text_of(&self, name: &str) -> Option<String> {
        self.child(name)
            .map(|c| clean_text(&c.text))
            .filter(|s| !s.is_empty())
    }
}

/// Parse up to `limit` episodes (feed order: newest first in practice; the
/// result is sorted by date when dates are known). `base` resolves relative
/// URLs.
/// Element nesting followed (real feeds go 5–6 levels deep): deeper tags are
/// treated as empty, so a hostile feed cannot make every end tag scan a
/// stack of millions.
const MAX_DEPTH: usize = 64;
/// Children kept per element of an item (enclosures, chapters, categories…).
const MAX_CHILDREN: usize = 256;

pub fn parse_feed(body: &[u8], base: &url::Url, limit: usize) -> Option<Feed> {
    let text = decode_text(body);
    if !looks_like_feed(text.as_bytes()) {
        return None;
    }
    let mut feed = Feed::default();
    let mut path: Vec<String> = Vec::new();
    let mut items: Vec<Node> = Vec::new();
    // Builder stack for the item being collected.
    let mut item_stack: Vec<Node> = Vec::new();
    // Over-collect a little: some feeds are not in date order.
    let collect_cap = (limit * 3).clamp(limit, 60);

    for tok in Tokenizer::new(&text) {
        match tok {
            Token::Start {
                name,
                attrs,
                self_closing,
            } => {
                if !item_stack.is_empty() || name == "item" || name == "entry" {
                    let node = Node {
                        name: name.clone(),
                        attrs,
                        ..Default::default()
                    };
                    if self_closing || item_stack.len() >= MAX_DEPTH {
                        if let Some(parent) = item_stack.last_mut() {
                            if parent.children.len() < MAX_CHILDREN {
                                parent.children.push(node);
                            }
                        }
                    } else {
                        item_stack.push(node);
                    }
                    continue;
                }
                // Channel level.
                let parent_is_channel = matches!(
                    path.last().map(String::as_str),
                    Some("channel") | Some("feed")
                );
                if parent_is_channel && name == "itunes:image" {
                    if let Some(href) = attr(&attrs, "href") {
                        feed.artwork_url = resolve(base, href).or(feed.artwork_url.take());
                    }
                }
                if !self_closing && path.len() < MAX_DEPTH {
                    path.push(name);
                }
            }
            Token::End { name } => {
                if !item_stack.is_empty() {
                    // Close up to the matching element (tolerates unclosed children).
                    if let Some(pos) = item_stack.iter().rposition(|n| n.name == name) {
                        while item_stack.len() > pos + 1 {
                            let child = item_stack.pop().unwrap();
                            let parent = item_stack.last_mut().unwrap();
                            if parent.children.len() < MAX_CHILDREN {
                                parent.children.push(child);
                            }
                        }
                        let node = item_stack.pop().unwrap();
                        match item_stack.last_mut() {
                            Some(parent) => {
                                if parent.children.len() < MAX_CHILDREN {
                                    parent.children.push(node);
                                }
                            }
                            None => {
                                items.push(node);
                                if items.len() >= collect_cap {
                                    break;
                                }
                            }
                        }
                    }
                    continue;
                }
                if let Some(pos) = path.iter().rposition(|n| *n == name) {
                    path.truncate(pos);
                }
            }
            Token::Text(t) => {
                if let Some(node) = item_stack.last_mut() {
                    node.text.push_str(&t);
                    continue;
                }
                let n = path.len();
                if n < 2 {
                    continue;
                }
                let leaf = path[n - 1].as_str();
                let parent = path[n - 2].as_str();
                if leaf == "title" && matches!(parent, "channel" | "feed") && feed.title.is_none() {
                    let s = clean_text(&t);
                    if !s.is_empty() {
                        feed.title = Some(s);
                    }
                } else if leaf == "url" && parent == "image" && n >= 3 && path[n - 3] == "channel" {
                    if feed.artwork_url.is_none() {
                        feed.artwork_url = resolve(base, t.trim());
                    }
                } else if (leaf == "logo" || leaf == "icon")
                    && parent == "feed"
                    && (feed.artwork_url.is_none() || leaf == "logo")
                {
                    if let Some(u) = resolve(base, t.trim()) {
                        feed.artwork_url = Some(u);
                    }
                }
            }
        }
    }

    let mut episodes: Vec<Episode> = items.iter().filter_map(|n| episode_from(n, base)).collect();
    // Newest first when dates are known; undated items keep feed order.
    if episodes.iter().all(|e| e.published_at.is_some()) {
        episodes.sort_by(|a, b| b.published_at.cmp(&a.published_at));
    }
    let mut seen = std::collections::HashSet::new();
    episodes.retain(|e| seen.insert(e.key.clone()));
    episodes.truncate(limit);
    feed.episodes = episodes;
    Some(feed)
}

fn attr<'a>(attrs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.trim())
        .filter(|v| !v.is_empty())
}

pub fn resolve(base: &url::Url, raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let u = base.join(raw).ok()?;
    matches!(u.scheme(), "http" | "https").then(|| u.to_string())
}

fn is_media_type(t: &str) -> bool {
    let t = t.to_ascii_lowercase();
    t.starts_with("audio/") || t.starts_with("video/") || t == "application/ogg"
}

fn looks_like_audio_url(u: &str) -> bool {
    let path = u
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    [
        ".mp3", ".m4a", ".aac", ".ogg", ".opus", ".oga", ".mp4", ".wav", ".flac",
    ]
    .iter()
    .any(|ext| path.ends_with(ext))
}

/// The playable file of an item: `<enclosure>`, Atom `<link rel=enclosure>`,
/// Media RSS `<media:content>` (also inside `<media:group>`). Audio wins over video.
fn enclosure(item: &Node, base: &url::Url) -> Option<(String, Option<String>)> {
    let mut found: Vec<(String, Option<String>)> = Vec::new();
    for e in item.children("enclosure") {
        if let Some(u) = e.attr("url").and_then(|u| resolve(base, u)) {
            found.push((u, e.attr("type").map(str::to_string)));
        }
    }
    for l in item.children("link") {
        if l.attr("rel") == Some("enclosure") {
            if let Some(u) = l.attr("href").and_then(|u| resolve(base, u)) {
                found.push((u, l.attr("type").map(str::to_string)));
            }
        }
    }
    let media = item.children("media:content").chain(
        item.children("media:group")
            .flat_map(|g| g.children("media:content")),
    );
    for m in media {
        let ty = m.attr("type").map(str::to_string);
        let medium = m.attr("medium").unwrap_or("");
        if medium == "image" || ty.as_deref().is_some_and(|t| t.starts_with("image/")) {
            continue;
        }
        if let Some(u) = m.attr("url").and_then(|u| resolve(base, u)) {
            found.push((u, ty));
        }
    }
    let rank = |(u, t): &(String, Option<String>)| -> u8 {
        match t.as_deref() {
            Some(t) if t.to_ascii_lowercase().starts_with("audio/") => 0,
            Some(t) if is_media_type(t) => 2,
            _ if looks_like_audio_url(u) => 1,
            Some(_) => 4,
            None => 3,
        }
    };
    found.sort_by_key(rank);
    found.into_iter().find(|f| rank(f) < 4)
}

fn item_artwork(item: &Node, base: &url::Url) -> Option<String> {
    if let Some(u) = item
        .child("itunes:image")
        .and_then(|i| i.attr("href").or_else(|| i.attr("url")))
    {
        return resolve(base, u);
    }
    if let Some(u) = item.child("media:thumbnail").and_then(|i| i.attr("url")) {
        return resolve(base, u);
    }
    for m in item.children("media:content") {
        let image = m.attr("medium") == Some("image")
            || m.attr("type").is_some_and(|t| t.starts_with("image/"));
        if image {
            if let Some(u) = m.attr("url") {
                return resolve(base, u);
            }
        }
    }
    None
}

fn episode_from(item: &Node, base: &url::Url) -> Option<Episode> {
    let (media_url, mime) = enclosure(item, base)?;
    let title = item
        .text_of("title")
        .or_else(|| item.text_of("itunes:title"))
        .unwrap_or_else(|| "—".into());
    let guid = item
        .text_of("guid")
        .or_else(|| item.text_of("id"))
        .unwrap_or_else(|| media_url.clone());
    let published_at = ["pubdate", "published", "dc:date", "updated", "issued"]
        .iter()
        .find_map(|k| item.text_of(k).and_then(|s| parse_date(&s)))
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    let duration_secs = ["itunes:duration", "duration", "podcast:duration"]
        .iter()
        .find_map(|k| item.text_of(k).and_then(|s| parse_duration(&s)))
        .or_else(|| {
            item.children("media:content")
                .find_map(|m| m.attr("duration").and_then(parse_duration))
        });
    let page_url = item
        .children("link")
        .find(|l| l.attr("rel").is_none_or(|r| r == "alternate"))
        .and_then(|l| {
            l.attr("href")
                .map(str::to_string)
                .or_else(|| Some(l.text.trim().to_string()).filter(|s| !s.is_empty()))
        })
        .and_then(|u| resolve(base, &u));
    Some(Episode {
        key: episode_key(&guid),
        title,
        published_at,
        duration_secs,
        media_url: Some(media_url),
        page_url,
        artwork_url: item_artwork(item, base),
        mime: mime.filter(|m| is_media_type(m)),
    })
}

/// `3725`, `3725.4`, `62:05`, `1:02:05`, `01:02:05.300`. Zero / garbage → None.
pub fn parse_duration(raw: &str) -> Option<u32> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    let secs: f64 = if s.contains(':') {
        let parts: Vec<&str> = s.split(':').map(str::trim).collect();
        if parts.len() > 3 {
            return None;
        }
        let mut total = 0f64;
        for p in &parts {
            let v: f64 = p.parse().ok()?;
            if v < 0.0 {
                return None;
            }
            total = total * 60.0 + v;
        }
        total
    } else {
        s.parse().ok()?
    };
    if !secs.is_finite() || !(1.0..=7.0 * 24.0 * 3600.0).contains(&secs) {
        return None;
    }
    Some(secs.round() as u32)
}

/// RFC 3339, RFC 2822 (also with named zones like `CEST` or a wrong
/// weekday), and a few bare formats (taken as UTC).
pub fn parse_date(raw: &str) -> Option<DateTime<Utc>> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(d) = DateTime::parse_from_rfc3339(s) {
        return Some(d.with_timezone(&Utc));
    }
    if let Ok(d) = DateTime::parse_from_rfc2822(s) {
        return Some(d.with_timezone(&Utc));
    }
    // Named zones chrono does not know, and a weekday that may be wrong.
    let mut t = s.to_string();
    if let Some((_, rest)) = t.split_once(',') {
        t = rest.trim().to_string();
    }
    let zones = [
        ("CEST", "+0200"),
        ("CET", "+0100"),
        ("BST", "+0100"),
        ("EEST", "+0300"),
        ("EET", "+0200"),
        ("IST", "+0530"),
        ("AEST", "+1000"),
        ("AEDT", "+1100"),
        ("PDT", "-0700"),
        ("PST", "-0800"),
        ("MDT", "-0600"),
        ("MST", "-0700"),
        ("CDT", "-0500"),
        ("CST", "-0600"),
        ("EDT", "-0400"),
        ("EST", "-0500"),
        ("UTC", "+0000"),
        ("GMT", "+0000"),
        ("Z", "+0000"),
    ];
    for (name, off) in zones {
        if let Some(stripped) = t.strip_suffix(name) {
            t = format!("{}{}", stripped.trim_end(), format_args!(" {off}"));
            break;
        }
    }
    for fmt in [
        "%d %b %Y %H:%M:%S %z",
        "%d %b %Y %H:%M %z",
        "%d %B %Y %H:%M:%S %z",
        "%Y-%m-%d %H:%M:%S %z",
    ] {
        if let Ok(d) = DateTime::parse_from_str(&t, fmt) {
            return Some(d.with_timezone(&Utc));
        }
    }
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%d %b %Y %H:%M:%S",
    ] {
        if let Ok(d) = NaiveDateTime::parse_from_str(&t, fmt) {
            return Some(Utc.from_utc_datetime(&d));
        }
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(&t, "%Y-%m-%d") {
        return Some(Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0)?));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_nesting_stays_linear_and_bounded() {
        let base = url::Url::parse("https://example.com/feed.xml").unwrap();
        let body = format!(
            "<rss><channel>{}{}</channel></rss>",
            "<a>".repeat(200_000),
            "</b>".repeat(200_000)
        );
        let t = std::time::Instant::now();
        let _ = parse_feed(body.as_bytes(), &base, 10);
        assert!(
            t.elapsed() < std::time::Duration::from_secs(5),
            "{:?}",
            t.elapsed()
        );

        let body = format!(
            "<rss><channel><item><title>x</title><enclosure url=\"https://example.com/a.mp3\" type=\"audio/mpeg\"/>{}{}</item></channel></rss>",
            "<a/>".repeat(100_000),
            "<d>".repeat(100_000)
        );
        let t = std::time::Instant::now();
        let feed = parse_feed(body.as_bytes(), &base, 10).unwrap();
        assert!(
            t.elapsed() < std::time::Duration::from_secs(5),
            "{:?}",
            t.elapsed()
        );
        assert_eq!(feed.episodes.len(), 1);
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("3725"), Some(3725));
        assert_eq!(parse_duration("3725.6"), Some(3726));
        assert_eq!(parse_duration("62:05"), Some(3725));
        assert_eq!(parse_duration("1:02:05"), Some(3725));
        assert_eq!(parse_duration("01:02:05.300"), Some(3725));
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("0"), None);
        assert_eq!(parse_duration("abc"), None);
        assert_eq!(parse_duration("1:2:3:4"), None);
    }

    #[test]
    fn dates() {
        let d = |s: &str| parse_date(s).map(|d| d.to_rfc3339());
        assert_eq!(
            d("Wed, 07 Oct 2026 13:00:00 GMT").as_deref(),
            Some("2026-10-07T13:00:00+00:00")
        );
        assert_eq!(
            d("Wed, 07 Oct 2026 15:00:00 +0200").as_deref(),
            Some("2026-10-07T13:00:00+00:00")
        );
        assert_eq!(
            d("Mon, 07 Oct 2026 15:00:00 CEST").as_deref(),
            Some("2026-10-07T13:00:00+00:00")
        );
        assert_eq!(
            d("2026-10-07T13:00:00Z").as_deref(),
            Some("2026-10-07T13:00:00+00:00")
        );
        assert_eq!(
            d("2026-10-07T13:03:58.573").as_deref(),
            Some("2026-10-07T13:03:58.573+00:00")
        );
        assert!(d("yesterday").is_none());
    }
}
