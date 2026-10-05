//! Display text derived from files and tags: track / album titles, track
//! numbers guessed from file names, release dates, genre tokens and the
//! accent folding used by search.
//!
//! Titles follow legacy `server/albumInfo.mjs` (`sanitizeLocalTrackTitleDisplay`),
//! except that version suffixes a listener cares about (remix, live, acoustic,
//! instrumental, …) are kept: only packaging cruft is stripped.

use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("static regex"))
}

/// `(feat. X)`, `(with X)`, `(X & Y)`: credits, never stripped.
fn is_collab_parens(inner: &str) -> bool {
    static A: OnceLock<Regex> = OnceLock::new();
    static B: OnceLock<Regex> = OnceLock::new();
    static C: OnceLock<Regex> = OnceLock::new();
    let t = inner.trim();
    if t.is_empty() {
        return false;
    }
    re(
        &A,
        r"(?i)^feat\.?|^ft[.\s]|^with\s+|\bfeatur(?:ing|e)\b|^\s*con\s+\w",
    )
    .is_match(t)
        || re(&B, r#"(?i)\b(?:feat|ft)\.?\s+[A-Za-zÀ-ÿ"'’]"#).is_match(t)
        || re(&C, r#"(?i)[,&]\s*[\w"'’ -]+\s+(?:&|feat|and)\b"#).is_match(t)
}

/// Packaging cruft inside round brackets (legacy `JUNK_PAREN_PATTERNS`, minus
/// the musical versions: remix, live, acoustic, unplugged, piano, orchestra,
/// instrumental, karaoke, a cappella, demo, cover, club/radio/extended edits).
fn is_junk_parens(inner: &str) -> bool {
    static JUNK: OnceLock<Vec<Regex>> = OnceLock::new();
    if is_collab_parens(inner) || inner.trim().is_empty() {
        return false;
    }
    let patterns = JUNK.get_or_init(|| {
        [
            r"(?i)\bofficial(\s*audio|\s*video|\s*music)?\b",
            r"(?i)\boriginal(\s*mix)?\b",
            r"(?i)\boriginal\s*version\b",
            r"(?i)\borig\.?(\s|$)",
            r"(?i)\bremaster(ed|ing)?\b",
            r"(?i)\bre-?master",
            r"(?i)\bvisuali[sz]er\b",
            r"(?i)\b(?:lyric|lyrics?)\s*video",
            r"(?i)^\s*lyrics?\s*$",
            r"(?i)^\s*music\s*video\s*$",
            r"(?i)\b(?:4k|uhd|h\.?265|h\.?264|2160p|1080p|720p)\b",
            r"(?i)(?:^|[^\d])\bhd\b|^\s*hd\s*$",
            r"(?i)\bmusic\s*video\b",
            r"(?i)^\s*(?:video|audio|clip)\s*$",
            r"(?i)\b(?:official|lyric|music|hq|hd)\s+(?:video|audio|clip)\b",
            r"(?i)\baudio\s*only",
            r"(?i)\bfrom\s+the\b",
            r"(?i)\bsoundtrack|^\s*ost\s*$",
            r"(?i)\btrailer|teaser|preview\b",
            r"(?i)\b(?:deluxe|explicit)\b",
            r"(?i)\b(?:amazon|apple\s*music|youtub(?:e|e\s*music|e\s*topic)?|spotify|deezer|tidal|vevo|soundcloud|pandora|iheartradio|shazam|napster|bandcamp)(?:\s*music)?\b",
            r"(?i)^\s*mv\s*$",
            r"(?i)\bbts\b|behind\s+the\s+scenes",
            r"(?i)\bmono\b|\bstereo\b|lossless|high[-\s]*quality|^\s*hq\s*$",
            r"(?i)^\s*edit\s*$",
            r"(?i)^\s*mix\s*$",
            r"(?i)^\s*version\s*$",
            r"(?i)\b\d{4}\s*remaster",
            r"(?i)re-?(?:issue|press|press(?:ing|ed)|cut)\b",
            r"(?i)m/v\b",
        ]
        .iter()
        .map(|p| Regex::new(p).expect("junk regex"))
        .collect()
    });
    patterns.iter().any(|r| r.is_match(inner))
}

fn collapse_spaces(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_junk_round_parens(s: &str) -> String {
    static PAREN: OnceLock<Regex> = OnceLock::new();
    let paren = re(&PAREN, r"\s*([\(（])([^)）]+)([\)）])");
    let mut t = s.to_string();
    for _ in 0..15 {
        let before = t.clone();
        t = paren
            .replace_all(&t, |caps: &regex::Captures<'_>| {
                if is_junk_parens(&caps[2]) {
                    " ".to_string()
                } else {
                    caps[0].to_string()
                }
            })
            .into_owned();
        t = collapse_spaces(&t);
        if t == before {
            break;
        }
    }
    t
}

fn strip_tail_youtube_cruft(s: &str) -> String {
    static TOPIC: OnceLock<Regex> = OnceLock::new();
    static PIPE_TOPIC: OnceLock<Regex> = OnceLock::new();
    let mut t = strip_junk_round_parens(&collapse_spaces(s));
    for _ in 0..4 {
        let before = t.clone();
        t = re(&TOPIC, r"(?i)\s*-\s*topic\s*$")
            .replace(&t, "")
            .trim()
            .to_string();
        t = re(&PIPE_TOPIC, r"(?i)\s*\|\s*[^|]+\s*-\s*Topic\s*$")
            .replace(&t, "")
            .trim()
            .to_string();
        t = collapse_spaces(&t);
        if t == before {
            break;
        }
    }
    t
}

fn artist_name_variants(artist: &str) -> Vec<String> {
    let a = artist.trim();
    if a.chars().count() < 2 {
        return Vec::new();
    }
    let mut out = vec![a.to_string()];
    let lower = a.to_lowercase();
    if lower.starts_with("the ") {
        out.push(a[4..].trim().to_string());
    } else {
        out.push(format!("The {a}"));
    }
    out.retain(|x| x.chars().count() >= 2);
    out
}

fn strip_artist_lead(s: &str, artist: &str) -> String {
    for v in artist_name_variants(artist) {
        let pattern = format!(r"(?i)^\s*{}\s*[-–—|]\s*", regex::escape(&v));
        if let Ok(r) = Regex::new(&pattern) {
            if r.is_match(s) {
                return r.replace(s, "").trim().to_string();
            }
        }
    }
    s.to_string()
}

fn strip_artist_trail(s: &str, artist: &str) -> String {
    for v in artist_name_variants(artist) {
        let pattern = format!(
            r"(?i)^(.*)\s*[-–—|]\s*{}(?:\s+(\([^)]+\)))?\s*$",
            regex::escape(&v)
        );
        let Ok(r) = Regex::new(&pattern) else {
            continue;
        };
        if let Some(caps) = r.captures(s) {
            let left = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
            if left.is_empty() {
                return s.to_string();
            }
            return match caps.get(2) {
                Some(extra) => collapse_spaces(&format!("{left} {}", extra.as_str().trim())),
                None => left.to_string(),
            };
        }
    }
    s.to_string()
}

/// Display title for a track whose only title is its file name (or a tag that
/// repeats it): drops `[…]`, leading track numbers (`01 - `, `01-`, `1. `),
/// packaging cruft in brackets, a trailing `- Topic` and the artist when it
/// leads or trails the name. `artist` is the track (or folder) artist.
pub fn sanitize_track_title(raw: &str, artist: Option<&str>) -> String {
    static BRACKETS: OnceLock<Regex> = OnceLock::new();
    static NUM_DASH: OnceLock<Regex> = OnceLock::new();
    static NUM_DOT: OnceLock<Regex> = OnceLock::new();
    let mut s = re(&BRACKETS, r"\[[^\]]*\]")
        .replace_all(raw, " ")
        .into_owned();
    s = collapse_spaces(&s);
    s = re(&NUM_DASH, r"^\d+\s*[-–—]\s*")
        .replace(&s, "")
        .trim()
        .to_string();
    s = re(&NUM_DOT, r"^\d+\s*\.\s+")
        .replace(&s, "")
        .trim()
        .to_string();
    s = strip_tail_youtube_cruft(&collapse_spaces(&s));
    let artist = artist.map(str::trim).unwrap_or("");
    s = strip_artist_lead(&s, artist);
    s = strip_artist_trail(&s, artist);
    s = collapse_spaces(&s);
    if s.chars().count() > 200 {
        s = s.chars().take(200).collect();
    }
    s
}

fn strip_number_prefix(s: &str) -> &str {
    static NUM: OnceLock<Regex> = OnceLock::new();
    let r = re(&NUM, r"^\s*\d{1,3}\s*(?:[-–—.]\s*|\s+)");
    match r.find(s) {
        Some(m) => &s[m.end()..],
        None => s,
    }
}

/// Title shown for a track: the tag title, unless the tag is missing or just
/// repeats the file name, in which case the file name is cleaned up.
pub fn track_display_title(tag_title: Option<&str>, file_stem: &str, artist: &str) -> String {
    let stem = file_stem.trim();
    let tag = tag_title.map(str::trim).filter(|t| !t.is_empty());
    let leading_digits = |s: &str| -> String {
        s.trim_start()
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect()
    };
    // A tag that repeats the file name, or carries the file's own "07 - "
    // numbering, is treated like the file name.
    let looks_like_file = |t: &str| {
        t.eq_ignore_ascii_case(stem)
            || (strip_number_prefix(t).len() != t.len()
                && leading_digits(t) == leading_digits(stem))
    };
    let source = match tag {
        Some(t) if !looks_like_file(t) => return t.to_string(),
        Some(t) => t,
        None => stem,
    };
    let cleaned = sanitize_track_title(source, Some(artist));
    if cleaned.is_empty() {
        source.to_string()
    } else {
        cleaned
    }
}

/// `(disc, track)` read from a file name: `07 - x`, `07. x`, `07 x`, and the
/// disc-track form `1-07 x`. Like legacy, at most two digits make a number.
pub fn guess_track_numbers(file_stem: &str) -> (Option<i64>, Option<i64>) {
    static DISC_TRACK: OnceLock<Regex> = OnceLock::new();
    static TRACK: OnceLock<Regex> = OnceLock::new();
    let s = file_stem.trim();
    if let Some(c) = re(&DISC_TRACK, r"^(\d)-(\d{2})(?:\s|[-–—._]|$)").captures(s) {
        let disc = c[1].parse::<i64>().ok().filter(|n| *n > 0);
        let track = c[2].parse::<i64>().ok().filter(|n| *n > 0);
        if track.is_some() {
            return (disc, track);
        }
    }
    let track = re(&TRACK, r"^(\d{1,2})(?:\s|[-–—._)]|$)")
        .captures(s)
        .and_then(|c| c[1].parse::<i64>().ok())
        .filter(|n| *n > 0);
    (None, track)
}

/// Full-width punctuation (used in folder names where `?` / `:` are not
/// allowed) back to ASCII.
pub fn fold_fullwidth_punct(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '？' => '?',
            '：' => ':',
            '！' => '!',
            '／' => '/',
            '＼' => '\\',
            '＂' => '"',
            '＇' => '\'',
            '＊' => '*',
            '＜' => '<',
            '＞' => '>',
            '｜' => '|',
            '（' => '(',
            '）' => ')',
            '，' => ',',
            '；' => ';',
            '＆' => '&',
            '＃' => '#',
            '\u{3000}' => ' ',
            other => other,
        })
        .collect()
}

/// Display form of an album title (tag or folder name): ASCII punctuation and
/// no `Album - ` prefix.
pub fn album_display_title(raw: &str) -> String {
    static PREFIX: OnceLock<Regex> = OnceLock::new();
    let s = collapse_spaces(&fold_fullwidth_punct(raw));
    let stripped = re(&PREFIX, r"(?i)^album\s*[-–—:]\s*").replace(&s, "");
    let out = stripped.trim();
    if out.is_empty() {
        s
    } else {
        out.to_string()
    }
}

/// Normalised release date: `YYYY-MM-DD`, `YYYY-MM` or `YYYY`. Accepts ISO
/// timestamps, `YYYYMMDD` (yt-dlp), `YYYY/MM/DD` and `DD-MM-YYYY` / `DD/MM/YYYY`.
pub fn normalize_date(raw: &str) -> Option<String> {
    static ISO: OnceLock<Regex> = OnceLock::new();
    static COMPACT: OnceLock<Regex> = OnceLock::new();
    static DMY: OnceLock<Regex> = OnceLock::new();
    static YEAR: OnceLock<Regex> = OnceLock::new();
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    let valid = |y: i64, m: Option<i64>, d: Option<i64>| -> Option<String> {
        if !(1000..=2999).contains(&y) {
            return None;
        }
        match (m, d) {
            (Some(m), Some(d)) if (1..=12).contains(&m) && (1..=31).contains(&d) => {
                Some(format!("{y:04}-{m:02}-{d:02}"))
            }
            (Some(m), None) if (1..=12).contains(&m) => Some(format!("{y:04}-{m:02}")),
            _ => Some(format!("{y:04}")),
        }
    };
    if let Some(c) = re(
        &ISO,
        r"^(\d{4})(?:[-/.](\d{1,2})(?:[-/.](\d{1,2}))?)?(?:[T\s].*)?$",
    )
    .captures(s)
    {
        let y = c[1].parse().ok()?;
        let m = c.get(2).and_then(|x| x.as_str().parse().ok());
        let d = c.get(3).and_then(|x| x.as_str().parse().ok());
        return valid(y, m, d);
    }
    if let Some(c) = re(&COMPACT, r"^(\d{4})(\d{2})(\d{2})$").captures(s) {
        return valid(c[1].parse().ok()?, c[2].parse().ok(), c[3].parse().ok());
    }
    if let Some(c) = re(&DMY, r"^(\d{1,2})[-/.](\d{1,2})[-/.](\d{4})$").captures(s) {
        return valid(c[3].parse().ok()?, c[2].parse().ok(), c[1].parse().ok());
    }
    re(&YEAR, r"^(\d{4})\b")
        .captures(s)
        .and_then(|c| c[1].parse().ok())
        .and_then(|y| valid(y, None, None))
}

/// 3 for a full date, 2 for year-month, 1 for a year, 0 otherwise.
pub fn date_precision(date: &str) -> u8 {
    match date.trim().len() {
        10 => 3,
        7 => 2,
        4 => 1,
        _ => 0,
    }
}

/// Generic genre stubs that are not genres.
fn is_stub_genre(lower: &str) -> bool {
    matches!(
        lower,
        "music"
            | "unknown"
            | "other"
            | "misc"
            | "miscellaneous"
            | "various"
            | "none"
            | "n/a"
            | "na"
            | "undefined"
            | "genre"
            | "null"
            | "unclassified"
            | "(null)"
            | "not classified"
    )
}

/// One genre string (`"Hip Hop; Pop Rap"`, `"Rock/Pop"`, `"a, b | c"`) to its
/// tokens, in order, without duplicates (case-insensitive) or junk (numbers,
/// ID3v1 `(17)` references, single letters, stubs like `Music`).
pub fn split_genres(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for part in raw.split([';', '/', ',', '|', '\0']) {
        let t = collapse_spaces(part.trim());
        if t.is_empty() {
            continue;
        }
        let digits = t.trim_matches(|c| c == '(' || c == ')');
        if digits.chars().all(|c| c.is_ascii_digit()) || t.chars().count() < 2 {
            continue;
        }
        if is_stub_genre(&t.to_lowercase()) {
            continue;
        }
        if seen.insert(genre_key(&t)) {
            out.push(t);
        }
    }
    out
}

/// Case-, accent-, space- and hyphen-insensitive key: "Hip Hop", "Hip-hop"
/// and "hiphop" share one.
pub fn genre_key(label: &str) -> String {
    fold_search(label)
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '-' | '_' | '.' | '\''))
        .collect()
}

fn title_case(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Pick the display label for each genre key out of the spellings seen
/// (`spelling → count`): the most frequent one that is not all lower-case,
/// preferring Title Case on ties; an all-lower-case genre gets Title Case.
pub fn canonical_genre_labels(
    spellings: &HashMap<String, HashMap<String, u64>>,
) -> HashMap<String, String> {
    let mut out = HashMap::with_capacity(spellings.len());
    for (key, variants) in spellings {
        let mut ranked: Vec<(&String, &u64)> = variants.iter().collect();
        ranked.sort_by(|(a, na), (b, nb)| {
            let lower_a = a.to_lowercase() == **a;
            let lower_b = b.to_lowercase() == **b;
            lower_a
                .cmp(&lower_b)
                .then(nb.cmp(na))
                .then_with(|| (title_case(b) == **b).cmp(&(title_case(a) == **a)))
                .then_with(|| a.cmp(b))
        });
        if let Some((best, _)) = ranked.first() {
            let label = if best.to_lowercase() == **best {
                title_case(best)
            } else {
                (*best).clone()
            };
            out.insert(key.clone(), label);
        }
    }
    out
}

/// Lower-case and drop the diacritics of Latin letters, for accent-insensitive
/// matching outside SQLite FTS.
pub fn fold_search(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in fold_fullwidth_punct(s).chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => out.push('a'),
            'ç' | 'ć' | 'č' => out.push('c'),
            'ď' | 'đ' => out.push('d'),
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => out.push('e'),
            'ğ' => out.push('g'),
            'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' | 'ı' => out.push('i'),
            'ł' | 'ľ' => out.push('l'),
            'ñ' | 'ń' | 'ň' => out.push('n'),
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => out.push('o'),
            'ř' => out.push('r'),
            'ś' | 'š' | 'ş' => out.push('s'),
            'ť' | 'ţ' => out.push('t'),
            'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => out.push('u'),
            'ý' | 'ÿ' => out.push('y'),
            'ź' | 'ż' | 'ž' => out.push('z'),
            'ß' => out.push_str("ss"),
            'æ' => out.push_str("ae"),
            'œ' => out.push_str("oe"),
            other => out.push(other),
        }
    }
    out
}

/// FTS5 query for free text: every word must match as a prefix, in any
/// indexed column. `None` when nothing searchable is left.
pub fn fts_query(q: &str) -> Option<String> {
    let terms: Vec<String> = q
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .take(12)
        .map(|t| format!("\"{}\"*", t.replace('"', "")))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genre_labels_prefer_frequent_mixed_case() {
        let mut m: HashMap<String, HashMap<String, u64>> = HashMap::new();
        let k = genre_key("Hip Hop");
        m.entry(k.clone()).or_default().insert("Hip-Hop".into(), 2);
        m.entry(k.clone()).or_default().insert("Hip Hop".into(), 5);
        m.entry(k.clone()).or_default().insert("hip hop".into(), 9);
        let labels = canonical_genre_labels(&m);
        assert_eq!(labels[&k], "Hip Hop");
        let mut lower: HashMap<String, HashMap<String, u64>> = HashMap::new();
        lower
            .entry("poprap".into())
            .or_default()
            .insert("pop rap".into(), 1);
        assert_eq!(canonical_genre_labels(&lower)["poprap"], "Pop Rap");
    }

    #[test]
    fn dates_keep_their_precision() {
        assert_eq!(normalize_date("2014-11-08").as_deref(), Some("2014-11-08"));
        assert_eq!(normalize_date("20141108").as_deref(), Some("2014-11-08"));
        assert_eq!(
            normalize_date("2014-11-08T10:00:00Z").as_deref(),
            Some("2014-11-08")
        );
        assert_eq!(normalize_date("2014-11").as_deref(), Some("2014-11"));
        assert_eq!(normalize_date("08/11/2014").as_deref(), Some("2014-11-08"));
        assert_eq!(normalize_date("1975").as_deref(), Some("1975"));
        assert_eq!(normalize_date("3"), None);
        assert_eq!(date_precision("2014-11-08"), 3);
        assert_eq!(date_precision("2014"), 1);
    }

    #[test]
    fn fts_query_quotes_terms() {
        assert_eq!(fts_query("hip hop").as_deref(), Some("\"hip\"* \"hop\"*"));
        assert_eq!(fts_query("  - "), None);
    }
}
