//! Deciding whether a provider result really is the local track / album.
//!
//! Pure functions (no network) so the thresholds are unit-tested with
//! recorded provider payloads. The rule of thumb: a search hit is accepted
//! only when BOTH the artist and the title are close to ours; a release
//! tracklist row is accepted on a close title (position helps, never alone).

use super::discogs::DiscogsTrackEntry;
use super::text::{artist_similarity, core_title, normalize_name, title_similarity};
use serde::Serialize;
use serde_json::Value;

/// Artist names must be at least this close (1.0 = same after normalization;
/// credits containing the artist score 0.9).
pub const MIN_ARTIST_SIMILARITY: f64 = 0.75;
/// Track titles from a search must be at least this close.
pub const MIN_TITLE_SIMILARITY: f64 = 0.8;
/// Album names this close mean "same release" (track numbers are trusted).
pub const MIN_ALBUM_SIMILARITY: f64 = 0.7;
/// Tracklist rows: title similarity needed without / with a position match.
pub const MIN_TRACKLIST_TITLE: f64 = 0.8;
pub const MIN_TRACKLIST_TITLE_WITH_POSITION: f64 = 0.6;

/// What we know about the local track.
#[derive(Debug, Clone, Default)]
pub struct TrackQuery {
    pub artist: String,
    /// Album name (folder or curated), may be empty.
    pub album: String,
    /// Cleaned current title (`prepare_track_title_for_meta`).
    pub title: String,
    /// Cleaned title derived from the file name.
    pub title_from_file: String,
    pub duration_ms: Option<i64>,
    /// Leading number of the file name (or the stored track number).
    pub track_number: Option<i64>,
    pub disc_number: Option<i64>,
}

/// One provider search hit, normalized.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackCandidate {
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub title: String,
    pub artist: String,
    pub album: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_number: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc_number: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// An accepted candidate with its scores.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoredTrack {
    pub candidate: TrackCandidate,
    pub title_score: f64,
    pub artist_score: f64,
    pub album_score: f64,
    /// Same release as the local album (its track numbers can be used).
    pub album_matches: bool,
    pub score: f64,
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Durations far apart (> 30 s and > 25 %) mean another version.
fn durations_conflict(a: Option<i64>, b: Option<i64>) -> bool {
    match (a.filter(|x| *x > 0), b.filter(|x| *x > 0)) {
        (Some(a), Some(b)) => {
            let diff = (a - b).abs();
            diff > 30_000 && (diff as f64) > (a.max(b) as f64) * 0.25
        }
        _ => false,
    }
}

fn mentions_live(s: &str) -> bool {
    let n = format!(" {} ", normalize_name(s));
    n.contains(" live ") || n.contains(" unplugged ") || n.contains(" in concert ")
}

/// Score one search hit; `None` when it must not be used.
pub fn score_track_candidate(q: &TrackQuery, c: &TrackCandidate) -> Option<ScoredTrack> {
    if c.title.trim().is_empty() {
        return None;
    }
    let artist_score = if q.artist.trim().is_empty() {
        0.0
    } else {
        artist_similarity(&q.artist, &c.artist)
    };
    if artist_score < MIN_ARTIST_SIMILARITY {
        return None;
    }
    let mut title_score = title_similarity(&q.title, &c.title);
    if !q.title_from_file.trim().is_empty() {
        title_score = title_score.max(title_similarity(&q.title_from_file, &c.title));
    }
    if title_score < MIN_TITLE_SIMILARITY {
        return None;
    }
    if durations_conflict(q.duration_ms, c.duration_ms) {
        return None;
    }
    let album_score = if q.album.trim().is_empty() || c.album.trim().is_empty() {
        0.0
    } else {
        title_similarity(&q.album, &c.album)
    };
    let album_matches = album_score >= MIN_ALBUM_SIMILARITY;
    let dur_bonus = match (q.duration_ms, c.duration_ms) {
        (Some(a), Some(b)) if a > 0 && b > 0 && (a - b).abs() <= 3_000 => 0.05,
        _ => 0.0,
    };
    // Prefer the exact spelling, and a studio version unless we asked for live.
    let raw_title = super::text::similarity(&q.title, &c.title)
        .max(super::text::similarity(&q.title_from_file, &c.title));
    let wants_live = mentions_live(&q.title) || mentions_live(&q.title_from_file);
    let live_penalty = if !wants_live && (mentions_live(&c.title) || mentions_live(&c.album)) {
        0.1
    } else {
        0.0
    };
    let score =
        title_score * 0.45 + raw_title * 0.05 + artist_score * 0.3 + album_score * 0.15 + dur_bonus
            - live_penalty;
    Some(ScoredTrack {
        candidate: c.clone(),
        title_score: round2(title_score),
        artist_score: round2(artist_score),
        album_score: round2(album_score),
        album_matches,
        score: round2(score),
    })
}

/// Best accepted candidate (never "the first row").
pub fn pick_best_track(q: &TrackQuery, rows: &[TrackCandidate]) -> Option<ScoredTrack> {
    rows.iter()
        .filter_map(|c| score_track_candidate(q, c))
        .max_by(|a, b| a.score.total_cmp(&b.score))
}

/// A tracklist row matched to the local file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TracklistMatch {
    pub index: usize,
    pub entry: DiscogsTrackEntry,
    pub title_score: f64,
    pub position_matches: bool,
}

/// Match the local track to a release tracklist by fuzzy title, with the
/// position as a tie-breaker / support (never enough on its own).
pub fn match_tracklist(q: &TrackQuery, tracklist: &[DiscogsTrackEntry]) -> Option<TracklistMatch> {
    let mut best: Option<(f64, TracklistMatch)> = None;
    for (i, row) in tracklist.iter().enumerate() {
        let mut ts = title_similarity(&q.title, &row.title);
        if !q.title_from_file.trim().is_empty() {
            ts = ts.max(title_similarity(&q.title_from_file, &row.title));
        }
        let pos = row.position.unwrap_or(i as i64 + 1);
        let disc_ok = q.disc_number.is_none_or(|d| d == row.disc);
        let position_matches = q.track_number == Some(pos) && disc_ok;
        let needed = if position_matches {
            MIN_TRACKLIST_TITLE_WITH_POSITION
        } else {
            MIN_TRACKLIST_TITLE
        };
        if ts < needed {
            continue;
        }
        if durations_conflict(q.duration_ms, row.duration_ms) {
            continue;
        }
        let score = ts + if position_matches { 0.15 } else { 0.0 };
        if best.as_ref().is_none_or(|(s, _)| score > *s) {
            best = Some((
                score,
                TracklistMatch {
                    index: i,
                    entry: row.clone(),
                    title_score: round2(ts),
                    position_matches,
                },
            ));
        }
    }
    best.map(|(_, m)| m)
}

// ---------------------------------------------------------------------------
// Provider payloads → candidates
// ---------------------------------------------------------------------------

fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_string()
}

fn opt_s(v: &Value, key: &str) -> Option<String> {
    Some(s(v, key)).filter(|x| !x.is_empty())
}

fn int(v: Option<&Value>) -> Option<i64> {
    let v = v?;
    v.as_i64()
        .or_else(|| v.as_f64().map(|f| f as i64))
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

/// Deezer `/search/track` (or `/track/{id}`) rows.
pub fn deezer_candidates(data: &Value) -> Vec<TrackCandidate> {
    let rows: Vec<&Value> = match data.get("data").and_then(Value::as_array) {
        Some(a) => a.iter().collect(),
        None if data.get("id").is_some() => vec![data],
        None => vec![],
    };
    rows.into_iter()
        .map(|r| TrackCandidate {
            source: "deezer".into(),
            id: int(r.get("id")).map(|n| n.to_string()),
            title: s(r, "title"),
            artist: r
                .pointer("/artist/name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            album: r
                .pointer("/album/title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            track_number: int(r.get("track_position")).filter(|n| *n > 0),
            disc_number: int(r.get("disk_number")).filter(|n| *n > 0),
            duration_ms: int(r.get("duration")).filter(|n| *n > 0).map(|s| s * 1000),
            release_date: opt_s(r, "release_date").or_else(|| {
                r.pointer("/album/release_date")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }),
            genre: None,
            url: opt_s(r, "link"),
        })
        .collect()
}

/// iTunes `/search?entity=song` rows.
pub fn itunes_track_candidates(data: &Value) -> Vec<TrackCandidate> {
    data.get("results")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter(|r| {
                    r.get("wrapperType").and_then(Value::as_str) == Some("track")
                        || r.get("kind").and_then(Value::as_str) == Some("song")
                })
                .map(|r| TrackCandidate {
                    source: "itunes".into(),
                    id: int(r.get("trackId")).map(|n| n.to_string()),
                    title: s(r, "trackName"),
                    artist: s(r, "artistName"),
                    album: s(r, "collectionName"),
                    track_number: int(r.get("trackNumber")).filter(|n| *n > 0),
                    disc_number: int(r.get("discNumber")).filter(|n| *n > 0),
                    duration_ms: int(r.get("trackTimeMillis")).filter(|n| *n > 0),
                    release_date: opt_s(r, "releaseDate").map(|d| d.chars().take(10).collect()),
                    genre: opt_s(r, "primaryGenreName"),
                    url: opt_s(r, "trackViewUrl").or_else(|| opt_s(r, "collectionViewUrl")),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// TheAudioDB `searchtrack.php` rows (descriptions are never used).
pub fn audiodb_track_candidates(data: &Value) -> Vec<TrackCandidate> {
    let rows: Vec<&Value> = match data.get("track") {
        Some(Value::Array(a)) => a.iter().collect(),
        Some(v @ Value::Object(_)) => vec![v],
        _ => vec![],
    };
    rows.into_iter()
        .map(|r| TrackCandidate {
            source: "theaudiodb".into(),
            id: opt_s(r, "idTrack"),
            title: s(r, "strTrack"),
            artist: s(r, "strArtist"),
            album: s(r, "strAlbum"),
            track_number: int(r.get("intTrackNumber")).filter(|n| *n > 0),
            disc_number: int(r.get("intCD")).filter(|n| *n > 0),
            duration_ms: int(r.get("intDuration")).filter(|n| *n > 0),
            release_date: None,
            genre: opt_s(r, "strGenre").or_else(|| opt_s(r, "strStyle")),
            url: None,
        })
        .collect()
}

/// MusicBrainz release (`inc=recordings`) → tracklist rows.
pub fn musicbrainz_tracklist(release: &Value) -> Vec<DiscogsTrackEntry> {
    let mut out = Vec::new();
    for medium in release
        .get("media")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let disc = int(medium.get("position")).unwrap_or(1);
        for tr in medium
            .get("tracks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let title = tr
                .get("title")
                .and_then(Value::as_str)
                .or_else(|| tr.pointer("/recording/title").and_then(Value::as_str))
                .unwrap_or("")
                .trim()
                .to_string();
            if title.is_empty() {
                continue;
            }
            out.push(DiscogsTrackEntry {
                disc,
                position: int(tr.get("position")).or(Some(out.len() as i64 + 1)),
                title,
                duration_ms: int(tr.get("length")).filter(|n| *n > 0),
            });
        }
    }
    out
}

/// Tie-break between MusicBrainz releases that match equally well: official
/// first, then the worldwide edition (`XW`), then the earliest date (a bare
/// year counts as its last day, so a precise earlier date wins). Lower sorts
/// first.
pub fn musicbrainz_release_rank(r: &Value) -> (u8, u8, String) {
    let official = r
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|s| s.eq_ignore_ascii_case("official"));
    let country = r.get("country").and_then(Value::as_str).or_else(|| {
        r.pointer("/release-events/0/area/iso-3166-1-codes/0")
            .and_then(Value::as_str)
    });
    let worldwide = country.is_some_and(|c| c.eq_ignore_ascii_case("XW"));
    let date = r
        .get("date")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|d| d.len() >= 4)
        .map(|d| match d.len() {
            4 => format!("{d}-12-31"),
            7 => format!("{d}-31"),
            _ => d.to_string(),
        })
        .unwrap_or_else(|| "9999-12-31".into());
    (u8::from(!official), u8::from(!worldwide), date)
}

/// Genres of a MusicBrainz release (its own, else its release group's), most
/// voted first, at most three, as `"Rock; Pop Rock"`. Not yet normalized.
pub fn musicbrainz_genres(release: &Value) -> Option<String> {
    let pick = |v: Option<&Value>| -> Vec<(i64, String)> {
        v.and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|g| {
                        let name = g.get("name").and_then(Value::as_str)?.trim();
                        (!name.is_empty()).then(|| {
                            (
                                g.get("count").and_then(Value::as_i64).unwrap_or(0),
                                name.to_string(),
                            )
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut genres = pick(release.get("genres"));
    if genres.is_empty() {
        genres = pick(release.pointer("/release-group/genres"));
    }
    // Most votes first; equal votes keep MusicBrainz's (alphabetical) order.
    genres.sort_by_key(|g| std::cmp::Reverse(g.0));
    let names: Vec<String> = genres.into_iter().take(3).map(|(_, n)| n).collect();
    (!names.is_empty()).then(|| names.join("; "))
}

/// Joined artist credit of a MusicBrainz release / recording.
pub fn musicbrainz_artist_credit(v: &Value) -> String {
    v.get("artist-credit")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|c| {
                    let name = c
                        .get("name")
                        .and_then(Value::as_str)
                        .or_else(|| c.pointer("/artist/name").and_then(Value::as_str))
                        .unwrap_or("");
                    format!(
                        "{name}{}",
                        c.get("joinphrase").and_then(Value::as_str).unwrap_or("")
                    )
                })
                .collect::<String>()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Albums
// ---------------------------------------------------------------------------

/// What we know about the local album.
#[derive(Debug, Clone, Default)]
pub struct AlbumQuery {
    pub artist: String,
    pub album: String,
    /// Audio files in the folder (0 = unknown).
    pub local_track_count: usize,
}

fn is_single_or_ep_title(title: &str) -> bool {
    let n = normalize_name(title);
    n.ends_with(" single") || n.ends_with(" ep") || n == "single" || n == "ep"
}

/// Score an album hit (`None` = reject): artist and album must both be
/// close, and a single / EP never matches a folder with 3+ tracks.
pub fn score_album_candidate(
    q: &AlbumQuery,
    artist: &str,
    title: &str,
    track_count: Option<i64>,
) -> Option<f64> {
    let a = if q.artist.trim().is_empty() {
        1.0
    } else {
        artist_similarity(&q.artist, artist)
    };
    if a < MIN_ARTIST_SIMILARITY {
        return None;
    }
    let t = title_similarity(&q.album, title).max(title_similarity(&q.album, &core_title(title)));
    if t < MIN_ALBUM_SIMILARITY {
        return None;
    }
    let local = q.local_track_count as i64;
    let tiny_release = is_single_or_ep_title(title) || track_count.is_some_and(|n| n <= 2);
    if local >= 3 && tiny_release && !is_single_or_ep_title(&q.album) {
        return None;
    }
    let mut score = t * 0.6 + a * 0.4;
    if let (Some(n), true) = (track_count, local > 0) {
        if n == local {
            score += 0.05;
        }
    }
    Some(round2(score))
}

/// Keep a fetched track count only when it does not contradict the folder by
/// a lot (an 8-track folder matched to a 1-track single keeps nothing).
pub fn sanitize_expected_track_count(expected: Option<i64>, local: usize) -> Option<i64> {
    let e = expected.filter(|n| *n > 0)?;
    let local = local as i64;
    if local <= 0 || e >= local {
        return Some(e);
    }
    let missing = local - e;
    let tolerance = 3.max((local as f64 * 0.34).ceil() as i64);
    (missing < tolerance).then_some(e)
}

/// True when the album name is just the folder name (no curated title yet).
pub fn album_name_is_folder_name(current: &str, folder: &str) -> bool {
    let c = current.trim();
    c.is_empty() || c == folder.trim() || normalize_name(c) == normalize_name(folder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn musicbrainz_prefers_official_worldwide_then_earliest() {
        let mut rels = [
            json!({ "id": "ar", "status": "Official", "country": "AR", "date": "2010" }),
            json!({ "id": "de", "status": "Official", "country": "DE", "date": "2010-09-10" }),
            json!({ "id": "us", "status": "Official", "country": "US", "date": "2010-09-14" }),
            json!({ "id": "boot", "status": "Bootleg", "country": "XW", "date": "2009" }),
        ];
        rels.sort_by_key(musicbrainz_release_rank);
        assert_eq!(rels[0]["id"], "de", "earliest precise official date");
        let mut with_xw = [
            json!({ "id": "de", "status": "Official", "country": "DE", "date": "2010-09-10" }),
            json!({ "id": "xw", "status": "Official", "date": "2010-09-13",
                    "release-events": [{ "area": { "iso-3166-1-codes": ["XW"] } }] }),
        ];
        with_xw.sort_by_key(musicbrainz_release_rank);
        assert_eq!(with_xw[0]["id"], "xw", "worldwide edition first");
    }

    #[test]
    fn musicbrainz_genres_fall_back_to_the_release_group() {
        let rel = json!({
            "genres": [],
            "release-group": { "genres": [
                { "name": "art rock", "count": 3 },
                { "name": "alternative rock", "count": 5 },
                { "name": "electronic", "count": 5 },
                { "name": "rock", "count": 2 }
            ] }
        });
        assert_eq!(
            musicbrainz_genres(&rel).as_deref(),
            Some("alternative rock; electronic; art rock")
        );
        assert_eq!(musicbrainz_genres(&json!({})), None);
    }

    fn eagles(title: &str) -> TrackQuery {
        TrackQuery {
            artist: "Eagles".into(),
            album: "Desperado".into(),
            title: title.into(),
            title_from_file: title.into(),
            ..Default::default()
        }
    }

    #[test]
    fn rejects_a_different_song_from_the_same_search() {
        let data = json!({"data": [{
            "id": 1, "title": "Heart of Gold", "duration": 187,
            "artist": {"name": "Neil Young"}, "album": {"title": "Harvest"},
            "link": "https://www.deezer.com/track/1"
        }]});
        let rows = deezer_candidates(&data);
        assert!(pick_best_track(&eagles("Twenty-One"), &rows).is_none());
    }

    #[test]
    fn accepts_same_song_with_edition_noise() {
        let data = json!({"data": [
            {"id": 2, "title": "Tequila Sunrise", "artist": {"name": "Neil Young"}, "album": {"title": "X"}},
            {"id": 3, "title": "Twenty-One (2013 Remaster)", "duration": 131,
             "artist": {"name": "Eagles"}, "album": {"title": "Desperado (2013 Remaster)"}}
        ]});
        let best = pick_best_track(&eagles("Twenty-One"), &deezer_candidates(&data)).unwrap();
        assert_eq!(best.candidate.id.as_deref(), Some("3"));
        assert!(best.album_matches);
    }

    #[test]
    fn studio_version_beats_live_version() {
        let data = json!({"data": [
            {"id": 1, "title": "Take It Easy (Live)", "artist": {"name": "Eagles"}, "album": {"title": "Golden Era Live"}},
            {"id": 2, "title": "Take It Easy (2013 Remaster)", "artist": {"name": "Eagles"}, "album": {"title": "Eagles"}}
        ]});
        let best = pick_best_track(&eagles("Take It Easy"), &deezer_candidates(&data)).unwrap();
        assert_eq!(best.candidate.id.as_deref(), Some("2"));
    }

    #[test]
    fn tracklist_needs_a_title_match() {
        let list = vec![
            DiscogsTrackEntry {
                disc: 1,
                position: Some(1),
                title: "Doolin-Dalton".into(),
                duration_ms: None,
            },
            DiscogsTrackEntry {
                disc: 1,
                position: Some(2),
                title: "Twenty-One".into(),
                duration_ms: None,
            },
        ];
        let mut q = eagles("Twenty One");
        q.track_number = Some(2);
        let m = match_tracklist(&q, &list).unwrap();
        assert_eq!(m.entry.position, Some(2));
        assert!(m.position_matches);
        // Position alone is not enough.
        let mut q = eagles("Something Else Entirely");
        q.track_number = Some(1);
        assert!(match_tracklist(&q, &list).is_none());
    }

    #[test]
    fn single_does_not_match_an_eight_track_folder() {
        let q = AlbumQuery {
            artist: "Aura".into(),
            album: "Aura Farming".into(),
            local_track_count: 8,
        };
        assert!(score_album_candidate(&q, "Aura", "Aura Farming - Single", Some(1)).is_none());
        assert!(score_album_candidate(&q, "Aura", "Aura Farming", Some(8)).is_some());
        assert!(score_album_candidate(&q, "Someone Else", "Aura Farming", Some(8)).is_none());
        assert_eq!(sanitize_expected_track_count(Some(1), 8), None);
        assert_eq!(sanitize_expected_track_count(Some(12), 15), Some(12));
        assert_eq!(sanitize_expected_track_count(Some(10), 8), Some(10));
    }
}
