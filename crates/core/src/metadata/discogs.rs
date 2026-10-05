//! Discogs release normalization, candidate scoring and tracklist → file
//! matching (parity legacy `server/discogsMetadata.mjs`,
//! `server/discogsTrackMatch.mjs`, `server/discogsApply.mjs`).

use crate::db::Track;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Minimum folder score for applying a release to an album folder.
pub const DISCOGS_APPLY_MIN_SCORE: f64 = 25.0;
/// Minimum title score for a tracklist row to be applied to a file.
const TRACK_MATCH_MIN_SCORE: i64 = 40;

const AUDIO_EXTS: &[&str] = &["mp3", "flac", "m4a", "ogg", "opus", "wav", "aac", "webm"];
const NON_MUSIC_FORMATS: &[&str] = &[
    "dvd",
    "bluray",
    "blu ray",
    "vhs",
    "umd",
    "interview",
    "documentary",
    "book",
    "poster",
    "merch",
];

/// One row of a Discogs tracklist (headings dropped).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscogsTrackEntry {
    pub disc: i64,
    /// Numeric position on its disc; `None` for vinyl sides (`A1`) and the like.
    pub position: Option<i64>,
    pub title: String,
    pub duration_ms: Option<i64>,
}

/// What applying a release writes on one track.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscogsTrackDelta {
    pub rel_path: String,
    pub track_number: i64,
    pub disc_number: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    /// Only set when the track had no real title (file-name fallback).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

fn fold_char(c: char) -> Option<char> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'č' => 'c',
        'ď' => 'd',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => 'e',
        'ğ' => 'g',
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' | 'ı' => 'i',
        'ł' | 'ľ' => 'l',
        'ñ' | 'ń' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ō' | 'ő' => 'o',
        'ř' => 'r',
        'ś' | 'š' | 'ş' => 's',
        'ť' | 'ţ' => 't',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => 'u',
        'ý' | 'ÿ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        c if c.is_ascii_alphanumeric() => c,
        _ => return None,
    })
}

/// Lowercase, accents folded, every other run of non-alphanumerics → one space.
fn norm(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_space = false;
    for c in s.chars().flat_map(char::to_lowercase) {
        match fold_char(c) {
            Some(f) => {
                if pending_space && !out.is_empty() {
                    out.push(' ');
                }
                pending_space = false;
                out.push(f);
            }
            None => pending_space = true,
        }
    }
    out
}

/// `normTitle`: [`norm`] after dropping an audio extension and a leading track number.
fn norm_title(s: &str) -> String {
    let mut t = s.trim().to_string();
    if let Some((stem, ext)) = t.rsplit_once('.') {
        if AUDIO_EXTS.iter().any(|e| ext.eq_ignore_ascii_case(e)) {
            t = stem.to_string();
        }
    }
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 {
        t = t[digits..]
            .trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '.' | '_' | '-'))
            .to_string();
    }
    norm(&t)
}

fn token_overlap(a: &str, b: &str) -> f64 {
    let tokens = |s: &str| -> std::collections::HashSet<String> {
        norm(s)
            .split(' ')
            .filter(|t| t.chars().count() > 1)
            .map(str::to_string)
            .collect()
    };
    let ta = tokens(a);
    let tb = tokens(b);
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let hit = ta.iter().filter(|t| tb.contains(*t)).count();
    hit as f64 / ta.len().max(tb.len()) as f64
}

fn value_text(v: Option<&Value>) -> String {
    match v {
        Some(Value::Array(a)) => a
            .iter()
            .map(|x| {
                x.as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| x.to_string())
            })
            .collect::<Vec<_>>()
            .join(" "),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

/// Score a search result (or a normalized release) for `artist` / `album`;
/// roughly 0–100, rounded to one decimal.
pub fn score_discogs_candidate(result: &Value, artist: &str, album: &str) -> f64 {
    let mut score = 0.0;
    let title = value_text(result.get("title"));
    match result.get("type").and_then(|v| v.as_str()) {
        Some("release") => score += 8.0,
        Some("master") => score += 4.0,
        _ => {}
    }
    let artist_field = match result.get("artist") {
        Some(v) if !v.is_null() => value_text(Some(v)),
        _ => title.clone(),
    };
    score += token_overlap(artist, &artist_field) * 40.0;

    let album_part = match title.split_once(" - ") {
        Some((_, rest)) => rest.to_string(),
        None => title.clone(),
    };
    score += token_overlap(album, &album_part) * 50.0;

    let fmt = norm(&value_text(result.get("format")));
    let fmt_padded = format!(" {fmt} ");
    if NON_MUSIC_FORMATS
        .iter()
        .any(|f| fmt_padded.contains(&format!(" {f} ")))
    {
        score -= 25.0;
    }

    let year = value_text(result.get("year"))
        .trim()
        .chars()
        .take(4)
        .collect::<String>()
        .parse::<i64>()
        .ok();
    if year.is_some_and(|y| y > 1900 && y < 2100) {
        score += 3.0;
    }
    (score * 10.0).round() / 10.0
}

/// Folder check before applying a release (legacy `scoreDiscogsReleaseForFolder`).
pub fn score_discogs_release_for_folder(release: &Value, artist: &str, album: &str) -> f64 {
    let title = release
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or(album);
    let fake = serde_json::json!({
        "title": title,
        "type": "release",
        "format": discogs_format_summary(release.get("formats")).map(|s| vec![s]).unwrap_or_default(),
        "year": release.get("year").cloned().or_else(|| release.get("released").cloned()),
        "artist": artist,
    });
    score_discogs_candidate(&fake, artist, album)
}

/// `"3:32"` → 212000, `"1:02:03"` → 3723000.
pub fn parse_discogs_duration_ms(dur: &str) -> Option<i64> {
    let s = dur.trim();
    if s.is_empty() {
        return None;
    }
    let parts: Option<Vec<i64>> = s.split(':').map(|p| p.trim().parse::<i64>().ok()).collect();
    match parts?.as_slice() {
        [m, sec] => Some((m * 60 + sec) * 1000),
        [h, m, sec] => Some((h * 3600 + m * 60 + sec) * 1000),
        _ => None,
    }
}

/// `"7"` → (None, 7); `"2-05"`, `"2.05"`, `"CD2-5"` → (2, 5); vinyl `"A1"` → None.
fn parse_position(pos: &str) -> Option<(Option<i64>, i64)> {
    let p = pos.trim();
    if p.is_empty() {
        return None;
    }
    if let Ok(n) = p.parse::<i64>() {
        return Some((None, n));
    }
    let lower = p.to_ascii_lowercase();
    let body = lower
        .strip_prefix("cd")
        .or_else(|| lower.strip_prefix("disc"))
        .unwrap_or(&lower)
        .trim();
    let (d, n) = body.split_once(['-', '.'])?;
    Some((Some(d.trim().parse().ok()?), n.trim().parse().ok()?))
}

/// Tracklist rows of a release (headings dropped).
pub fn tracklist_from_release(release: &Value) -> Vec<DiscogsTrackEntry> {
    let Some(rows) = release.get("tracklist").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for row in rows {
        if row.get("type_").and_then(|v| v.as_str()) == Some("heading") {
            continue;
        }
        let title = row
            .get("title")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .unwrap_or("");
        if title.is_empty() {
            continue;
        }
        let pos = value_text(row.get("position"));
        let parsed = parse_position(&pos);
        let disc = row
            .get("disc")
            .and_then(|v| v.as_i64())
            .or_else(|| parsed.and_then(|(d, _)| d))
            .unwrap_or(1);
        out.push(DiscogsTrackEntry {
            disc,
            position: parsed.map(|(_, n)| n),
            title: title.to_string(),
            duration_ms: row
                .get("duration")
                .and_then(|v| v.as_str())
                .and_then(parse_discogs_duration_ms),
        });
    }
    out
}

/// Split a stored genre string into unique parts (`;`, `/`, `,`), keeping order.
pub fn parse_genres(raw: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for part in raw.split([';', '/', ',']) {
        let p = part.trim();
        if p.is_empty() || !seen.insert(p.to_lowercase()) {
            continue;
        }
        out.push(p.to_string());
    }
    out
}

/// All genres and styles of a release, as `"Rock; Grunge"`.
pub fn genres_from_release(release: &Value) -> Option<String> {
    let mut merged: Vec<String> = Vec::new();
    for key in ["genres", "styles"] {
        if let Some(arr) = release.get(key).and_then(|v| v.as_array()) {
            merged.extend(arr.iter().filter_map(|g| g.as_str()).map(str::to_string));
        }
    }
    let parts = parse_genres(&merged.join(", "));
    (!parts.is_empty()).then(|| parts.join("; "))
}

pub fn discogs_format_summary(formats: Option<&Value>) -> Option<String> {
    let arr = formats?.as_array()?;
    let parts: Vec<String> = arr
        .iter()
        .filter_map(|f| {
            let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let desc = f
                .get("descriptions")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            let qty = f
                .get("qty")
                .and_then(|v| v.as_str())
                .filter(|q| *q != "1")
                .map(|q| format!(" x{q}"))
                .unwrap_or_default();
            let head = [name, desc.as_str()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
            (!head.is_empty()).then(|| format!("{head}{qty}"))
        })
        .collect();
    let s = parts.join(" · ");
    (!s.is_empty()).then(|| s.chars().take(300).collect())
}

/// Best tracklist row for one file (legacy `matchTrackToDiscogsEntry`):
/// exact normalized title 100, containment 70, else 15 per shared word, plus
/// 20 when the file name's leading number equals the row position. Below 40
/// there is no match.
pub fn match_track_to_discogs_entry<'a>(
    file_name: &str,
    title_raw: &str,
    tracklist: &'a [DiscogsTrackEntry],
    title_prepared: Option<&str>,
) -> Option<(usize, &'a DiscogsTrackEntry)> {
    let title = title_prepared
        .filter(|s| !s.trim().is_empty())
        .or(Some(title_raw).filter(|s| !s.trim().is_empty()))
        .unwrap_or(file_name);
    let n_file = norm_title(title);
    if n_file.is_empty() {
        return None;
    }
    let file_tokens: Vec<&str> = n_file.split(' ').collect();
    let stem = file_name
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(file_name);
    let lead: Option<i64> = {
        let digits: String = stem.chars().take_while(|c| c.is_ascii_digit()).collect();
        digits.parse().ok()
    };

    let mut best: Option<(usize, &DiscogsTrackEntry)> = None;
    let mut best_score = 0i64;
    for (i, row) in tracklist.iter().enumerate() {
        let n_row = norm_title(&row.title);
        if n_row.is_empty() {
            continue;
        }
        let mut score = if n_file == n_row {
            100
        } else if n_file.contains(&n_row) || n_row.contains(&n_file) {
            70
        } else {
            let row_tokens: Vec<&str> = n_row.split(' ').collect();
            file_tokens
                .iter()
                .filter(|t| row_tokens.contains(t))
                .count() as i64
                * 15
        };
        if row.position.is_some() && lead.is_some() && lead == row.position {
            score += 20;
        }
        if score > best_score {
            best_score = score;
            best = Some((i, row));
        }
    }
    (best_score >= TRACK_MATCH_MIN_SCORE)
        .then_some(best)
        .flatten()
}

fn file_name_of(rel_path: &str) -> &str {
    rel_path.rsplit('/').next().unwrap_or(rel_path)
}

fn file_stem_of(file_name: &str) -> &str {
    match file_name.rsplit_once('.') {
        Some((stem, ext)) if AUDIO_EXTS.iter().any(|e| ext.eq_ignore_ascii_case(e)) => stem,
        _ => file_name,
    }
}

/// Per-track changes for applying a release to an album's tracks
/// (legacy `enrichTracksFromDiscogsTracklist`).
pub fn plan_discogs_track_deltas(
    tracks: &[Track],
    artist: &str,
    tracklist: &[DiscogsTrackEntry],
    discogs_uri: Option<&str>,
) -> Vec<DiscogsTrackDelta> {
    if tracklist.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for t in tracks {
        let file_name = file_name_of(&t.rel_path);
        let title_raw = file_stem_of(file_name);
        let prepared = super::sanitize_local_track_title_display(title_raw, Some(artist), None);
        let hit = match_track_to_discogs_entry(file_name, title_raw, tracklist, Some(&prepared))
            .or_else(|| {
                // Tagged files: the tag title often matches when the name does not.
                match_track_to_discogs_entry(file_name, &t.title, tracklist, None)
            });
        let Some((index, row)) = hit else {
            continue;
        };
        let current = t.title.trim();
        let untitled = current.is_empty() || current == title_raw || current == prepared;
        out.push(DiscogsTrackDelta {
            rel_path: t.rel_path.clone(),
            track_number: row.position.unwrap_or(index as i64 + 1),
            disc_number: row.disc,
            duration_ms: row.duration_ms.filter(|d| *d > 0),
            title: untitled.then(|| row.title.clone()),
            source: "discogs".into(),
            url: discogs_uri.map(str::to_string),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry(position: i64, title: &str, duration_ms: i64) -> DiscogsTrackEntry {
        DiscogsTrackEntry {
            disc: 1,
            position: Some(position),
            title: title.into(),
            duration_ms: Some(duration_ms),
        }
    }

    fn nirvana() -> Vec<DiscogsTrackEntry> {
        vec![
            entry(1, "Smells Like Teen Spirit", 301_000),
            entry(2, "In Bloom", 254_000),
        ]
    }

    // discogsTrackMatch.test.ts
    #[test]
    fn matches_by_normalized_title() {
        let list = nirvana();
        let (_, row) = match_track_to_discogs_entry(
            "01 Smells Like Teen Spirit.flac",
            "Smells Like Teen Spirit",
            &list,
            Some("Smells Like Teen Spirit"),
        )
        .unwrap();
        assert_eq!(row.title, "Smells Like Teen Spirit");
        assert_eq!(row.position, Some(1));
    }

    #[test]
    fn matches_by_leading_track_number_in_filename() {
        let list = nirvana();
        let (_, row) =
            match_track_to_discogs_entry("02 - In Bloom.mp3", "In Bloom", &list, Some("In Bloom"))
                .unwrap();
        assert_eq!(row.title, "In Bloom");
    }

    #[test]
    fn unrelated_titles_do_not_match() {
        let list = nirvana();
        assert!(match_track_to_discogs_entry(
            "07 Something Else.mp3",
            "Something Else",
            &list,
            None
        )
        .is_none());
    }

    #[test]
    fn accents_and_punctuation_are_ignored() {
        let list = vec![entry(3, "Café Del Mar (Energy 52 Mix)", 0)];
        assert!(
            match_track_to_discogs_entry("03 cafe del mar energy 52 mix.mp3", "", &list, None)
                .is_some()
        );
    }

    // discogsMetadata.test.ts
    #[test]
    fn parses_mm_ss_durations() {
        assert_eq!(parse_discogs_duration_ms("3:32"), Some(212_000));
        assert_eq!(parse_discogs_duration_ms("1:02:03"), Some(3_723_000));
        assert_eq!(parse_discogs_duration_ms(""), None);
        assert_eq!(parse_discogs_duration_ms("x:10"), None);
    }

    #[test]
    fn scores_album_releases_above_interviews() {
        let album = score_discogs_candidate(
            &json!({
                "type": "release",
                "title": "Nirvana - Nevermind",
                "artist": "Nirvana",
                "year": "1991",
                "format": ["CD", "Album"],
            }),
            "Nirvana",
            "Nevermind",
        );
        let dvd = score_discogs_candidate(
            &json!({
                "type": "release",
                "title": "Nirvana - Nevermind",
                "artist": "Nirvana",
                "format": ["DVD", "Interview"],
            }),
            "Nirvana",
            "Nevermind",
        );
        assert!(album > dvd, "{album} vs {dvd}");
        assert!(album >= DISCOGS_APPLY_MIN_SCORE);
    }

    #[test]
    fn normalizes_release_tracklist_and_genres() {
        let release = json!({
            "id": 123,
            "title": "Test Album",
            "year": 2020,
            "genres": ["Rock"],
            "styles": ["Grunge", "rock"],
            "formats": [{"name": "Vinyl", "descriptions": ["LP"], "qty": "1"}],
            "tracklist": [
                {"type_": "heading", "title": "Side A"},
                {"type_": "track", "position": "1", "title": "One", "duration": "3:00"},
                {"type_": "track", "position": "2-05", "title": "Two", "duration": ""},
                {"type_": "track", "position": "B1", "title": "Three"},
            ],
        });
        let list = tracklist_from_release(&release);
        assert_eq!(list.len(), 3);
        assert_eq!(list[0], entry(1, "One", 180_000));
        assert_eq!((list[1].disc, list[1].position), (2, Some(5)));
        assert_eq!(list[2].position, None);
        assert_eq!(
            genres_from_release(&release).as_deref(),
            Some("Rock; Grunge")
        );
        assert_eq!(
            discogs_format_summary(release.get("formats")).as_deref(),
            Some("Vinyl, LP")
        );
    }

    fn track(rel: &str, title: &str) -> Track {
        Track {
            id: 0,
            rel_path: rel.into(),
            title: title.into(),
            artist_name: "Nirvana".into(),
            album_name: "Nevermind".into(),
            duration_ms: 0,
            track_number: None,
            album_id: None,
            artist_id: None,
            genre: None,
            release_date: None,
            lyrics: None,
            source: None,
            url: None,
            bpm: None,
        }
    }

    #[test]
    fn plans_track_numbers_durations_and_fallback_titles() {
        let list = nirvana();
        let tracks = vec![
            track("Nirvana/Nevermind/02 - in bloom.mp3", "02 - in bloom"),
            track(
                "Nirvana/Nevermind/teen spirit.flac",
                "Smells Like Teen Spirit",
            ),
            track("Nirvana/Nevermind/99 bonus.mp3", "99 bonus"),
        ];
        let deltas = plan_discogs_track_deltas(&tracks, "Nirvana", &list, Some("https://d/r/1"));
        assert_eq!(deltas.len(), 2);
        assert_eq!(deltas[0].track_number, 2);
        assert_eq!(deltas[0].duration_ms, Some(254_000));
        assert_eq!(
            deltas[0].title.as_deref(),
            Some("In Bloom"),
            "file-name title replaced"
        );
        assert_eq!(deltas[1].track_number, 1);
        assert_eq!(deltas[1].title, None, "a real tag title is kept");
        assert_eq!(deltas[1].url.as_deref(), Some("https://d/r/1"));
    }
}
