//! Reading what an audio file carries about itself: tags, the embedded
//! picture and (for the scan) its duration, in one open of the file.
//!
//! [lofty] reads MP3 (ID3v2, ID3v1, APE), FLAC (Vorbis comments, ID3v2),
//! Ogg Vorbis / Opus (Vorbis comments), MP4 / M4A / ALAC (ilst atoms), raw
//! AAC (ID3), WAV (ID3v2, RIFF INFO) and AIFF (ID3v2, text chunks). Pictures
//! are skipped unless asked for, so a huge cover never costs memory (or, as
//! before 5.1, makes the whole read fail: lofty refuses blocks over 16 MiB).
//! Containers lofty does not know (WebM / Matroska, WMA / ASF) are read from
//! `ffmpeg -i`, which the scan already ran on them for the duration.

use crate::db::text::{date_precision, normalize_date};
use lofty::config::{ParseOptions, ParsingMode};
use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::picture::PictureType;
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, Tag};
use std::path::Path;
use tracing::debug;

/// Values found in the file's tags. Text is trimmed; empty values are `None`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EmbeddedTags {
    pub title: Option<String>,
    /// Track artist(s), several values joined with `", "`.
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub track_number: Option<i64>,
    pub track_total: Option<i64>,
    pub disc_number: Option<i64>,
    pub disc_total: Option<i64>,
    /// `YYYY-MM-DD`, `YYYY-MM` or `YYYY`: the most precise date in the tags.
    pub date: Option<String>,
    /// Every genre value joined with `"; "` (split again for display).
    pub genre: Option<String>,
    pub bpm: Option<f64>,
    /// Unsynced lyrics, or LRC text (synced) when that is what the tag holds.
    pub lyrics: Option<String>,
    pub mb_recording_id: Option<String>,
    pub mb_release_id: Option<String>,
    pub mb_artist_id: Option<String>,
    pub mb_release_group_id: Option<String>,
}

impl EmbeddedTags {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A picture stored in the file.
#[derive(Clone)]
pub struct EmbeddedPicture {
    pub data: Vec<u8>,
    /// Marked as the front cover (else: the first picture of the file).
    pub front: bool,
}

impl std::fmt::Debug for EmbeddedPicture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddedPicture")
            .field("bytes", &self.data.len())
            .field("front", &self.front)
            .finish()
    }
}

/// What to read.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReadRequest {
    pub tags: bool,
    pub picture: bool,
    /// Duration and stream properties (the scan); the backfill job skips them.
    pub properties: bool,
}

#[derive(Debug, Default)]
pub struct FileRead {
    pub tags: EmbeddedTags,
    pub picture: Option<EmbeddedPicture>,
    /// Container duration in ms; 0 when unknown (or not asked for).
    pub duration_ms: i64,
    /// MPEG audio (MP3), whose duration the scan may have to count.
    pub is_mpeg: bool,
    /// lofty parsed the file; false when the values came from ffmpeg (or
    /// nothing could read it).
    pub parsed: bool,
}

fn options(req: ReadRequest, picture: bool) -> ParseOptions {
    ParseOptions::new()
        .parsing_mode(ParsingMode::BestAttempt)
        .read_properties(req.properties)
        .read_tags(req.tags || picture)
        .read_cover_art(picture)
}

fn lofty_read(path: &Path, opts: ParseOptions) -> lofty::error::Result<lofty::file::TaggedFile> {
    // The content decides the format (a `.m4a` that is really WebM, a FLAC
    // with an ID3 header); the extension is only the fallback.
    // A parser bug on one odd file must not take the scan (or the backfill,
    // which would retry the same batch at every start) down with it.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Probe::open(path)?.options(opts).guess_file_type()?.read()
    }))
    .unwrap_or_else(|_| {
        tracing::warn!(path = %path.display(), "tag reader panicked; file skipped");
        Err(lofty::error::LoftyError::new(
            lofty::error::ErrorKind::UnknownFormat,
        ))
    })
}

/// Read `path` once: tags and/or the picture and/or the properties.
pub fn read_file(path: &Path, req: ReadRequest) -> FileRead {
    let mut out = FileRead::default();
    let mut tagged = lofty_read(path, options(req, req.picture));
    if req.picture && tagged.is_err() {
        // A broken or oversized picture must not cost the tags.
        debug!(path = %path.display(), "read with pictures failed; retrying without");
        tagged = lofty_read(path, options(req, false));
    }
    match tagged {
        Ok(file) => {
            out.parsed = true;
            out.is_mpeg = file.file_type() == FileType::Mpeg;
            if req.properties {
                out.duration_ms = file.properties().duration().as_millis() as i64;
            }
            // Primary tag first (ID3v2, Vorbis comments, ilst), then the
            // others (ID3v1, APE, RIFF INFO) for whatever it lacks.
            let mut tags: Vec<&Tag> = Vec::new();
            if let Some(primary) = file.primary_tag() {
                tags.push(primary);
            }
            for t in file.tags() {
                if !tags.iter().any(|p| std::ptr::eq(*p, t)) {
                    tags.push(t);
                }
            }
            if req.tags {
                out.tags = tags_from(&tags);
            }
            if req.picture {
                out.picture = picture_from(&tags);
            }
        }
        Err(err) => {
            debug!(path = %path.display(), error = %err, "lofty cannot read file");
            if req.tags || req.picture || req.properties {
                if let Some(probe) = ffmpeg::probe(path) {
                    out.duration_ms = probe.duration_ms;
                    if req.tags {
                        out.tags = probe.tags;
                    }
                    if req.picture && probe.has_picture {
                        out.picture = ffmpeg::picture(path);
                    }
                }
            }
        }
    }
    out
}

/// Only the picture (an album without a cover whose tracks were not re-read).
pub fn read_picture(path: &Path) -> Option<EmbeddedPicture> {
    read_file(
        path,
        ReadRequest {
            tags: false,
            picture: true,
            properties: false,
        },
    )
    .picture
}

/// Artist that files in the library root are grouped by (`{artist}/Tracks`).
///
/// Frozen to what 5.0 read: the first artist of the primary (else first)
/// tag, the file type taken from the extension, nothing else. The group is
/// part of every track's `rel_path`, the key favorites, playlists and Studio
/// edits hang on: reading it the richer 5.1 way (all tags, ffmpeg, joined
/// multi-value artists) would move those tracks and detach their data.
pub fn group_artist(path: &Path) -> Option<String> {
    let tagged = Probe::open(path).ok()?.read().ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    tag.artist()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn clean(s: &str) -> Option<String> {
    let t = s.trim_matches(|c: char| c.is_whitespace() || c == '\0');
    (!t.is_empty()).then(|| t.to_string())
}

/// First non-empty value of `key` in the first tag that has one.
fn first(tags: &[&Tag], key: &ItemKey) -> Option<String> {
    tags.iter().find_map(|t| t.get_strings(key).find_map(clean))
}

/// Every value of `key` in the first tag that has any (multi-value Vorbis
/// fields), deduplicated case-insensitively.
fn all(tags: &[&Tag], key: &ItemKey) -> Vec<String> {
    for t in tags {
        let mut out: Vec<String> = Vec::new();
        for v in t.get_strings(key).filter_map(clean) {
            if !out.iter().any(|o| o.eq_ignore_ascii_case(&v)) {
                out.push(v);
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    Vec::new()
}

fn joined(values: Vec<String>, sep: &str) -> Option<String> {
    (!values.is_empty()).then(|| values.join(sep))
}

fn positive(n: Option<u32>) -> Option<i64> {
    n.filter(|n| *n > 0).map(i64::from)
}

/// `"4/11"` (Vorbis comments often hold the total in the number field,
/// which lofty's accessors do not parse) → (4, 11).
fn pair(tag: &Tag, key: &ItemKey) -> (Option<i64>, Option<i64>) {
    let Some(raw) = tag.get_string(key) else {
        return (None, None);
    };
    let mut parts = raw.split('/');
    let mut num = || {
        parts
            .next()
            .and_then(|p| p.trim().parse::<i64>().ok())
            .filter(|n| *n > 0)
    };
    (num(), num())
}

fn tags_from(tags: &[&Tag]) -> EmbeddedTags {
    let unknown = |k: &str| ItemKey::Unknown(k.to_string());
    let lyrics = first(tags, &ItemKey::Lyrics)
        .or_else(|| first(tags, &unknown("UNSYNCEDLYRICS")))
        .or_else(|| first(tags, &unknown("UNSYNCED LYRICS")));
    EmbeddedTags {
        title: first(tags, &ItemKey::TrackTitle),
        artist: joined(all(tags, &ItemKey::TrackArtist), ", "),
        album_artist: joined(all(tags, &ItemKey::AlbumArtist), ", "),
        album: first(tags, &ItemKey::AlbumTitle),
        track_number: tags
            .iter()
            .find_map(|t| positive(t.track()).or_else(|| pair(t, &ItemKey::TrackNumber).0)),
        track_total: tags
            .iter()
            .find_map(|t| positive(t.track_total()).or_else(|| pair(t, &ItemKey::TrackNumber).1)),
        disc_number: tags
            .iter()
            .find_map(|t| positive(t.disk()).or_else(|| pair(t, &ItemKey::DiscNumber).0)),
        disc_total: tags
            .iter()
            .find_map(|t| positive(t.disk_total()).or_else(|| pair(t, &ItemKey::DiscNumber).1)),
        date: best_date(tags),
        genre: joined(all(tags, &ItemKey::Genre), "; "),
        bpm: tags.iter().find_map(|t| read_bpm(t)),
        lyrics,
        mb_recording_id: first(tags, &ItemKey::MusicBrainzRecordingId),
        mb_release_id: first(tags, &ItemKey::MusicBrainzReleaseId),
        mb_artist_id: first(tags, &ItemKey::MusicBrainzArtistId),
        mb_release_group_id: first(tags, &ItemKey::MusicBrainzReleaseGroupId),
    }
}

fn picture_from(tags: &[&Tag]) -> Option<EmbeddedPicture> {
    let pictures = || {
        tags.iter()
            .flat_map(|t| t.pictures().iter())
            .filter(|p| !p.data().is_empty())
    };
    if let Some(p) = pictures().find(|p| p.pic_type() == PictureType::CoverFront) {
        return Some(EmbeddedPicture {
            data: p.data().to_vec(),
            front: true,
        });
    }
    pictures().next().map(|p| EmbeddedPicture {
        data: p.data().to_vec(),
        front: false,
    })
}

/// `"128"`, `"127.5"`, `"127,5"`, `"120 BPM"` → tempo; nonsense → None.
pub fn parse_bpm(raw: &str) -> Option<f64> {
    let t = raw.trim().replace(',', ".");
    let num: String = t
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let v: f64 = num.parse().ok()?;
    (v.is_finite() && v > 0.0 && v < 1000.0).then(|| (v * 100.0).round() / 100.0)
}

/// Tempo from ID3 `TBPM` / MP4 `tmpo` (integer BPM), Vorbis/APE `BPM`, or a
/// free-form `TEMPO` field.
fn read_bpm(tag: &Tag) -> Option<f64> {
    [
        ItemKey::Bpm,
        ItemKey::IntegerBpm,
        ItemKey::Unknown("TEMPO".into()),
        ItemKey::Unknown("tempo".into()),
    ]
    .iter()
    .find_map(|k| tag.get_string(k).and_then(parse_bpm))
}

/// Most precise release date of the first tag that has one: recording /
/// release dates are full dates more often than the year field (yt-dlp
/// writes `YYYYMMDD` there).
fn best_date(tags: &[&Tag]) -> Option<String> {
    tags.iter().find_map(|t| read_tag_date(t))
}

fn read_tag_date(tag: &Tag) -> Option<String> {
    let mut best: Option<String> = None;
    for key in &[
        ItemKey::RecordingDate,
        ItemKey::ReleaseDate,
        ItemKey::OriginalReleaseDate,
        ItemKey::Year,
    ] {
        let Some(date) = tag.get_string(key).and_then(normalize_date) else {
            continue;
        };
        if best
            .as_deref()
            .is_none_or(|b| date_precision(&date) > date_precision(b))
        {
            best = Some(date);
        }
    }
    best.or_else(|| {
        tag.year()
            .filter(|y| *y > 0)
            .and_then(|y| normalize_date(&y.to_string()))
    })
}

/// Duration (and tags, pictures) through ffmpeg, for containers lofty does
/// not read. Runs only on those files, on the scan's blocking thread.
pub(crate) mod ffmpeg {
    use super::{clean, parse_bpm, EmbeddedPicture, EmbeddedTags};
    use crate::db::text::{date_precision, normalize_date};
    use std::path::Path;
    use std::process::{Command, Stdio};

    pub struct Probe {
        pub duration_ms: i64,
        pub tags: EmbeddedTags,
        /// The file has a video stream (an attached picture in audio files).
        pub has_picture: bool,
    }

    fn binary() -> Option<std::path::PathBuf> {
        let ffmpeg = crate::tools::resolve_blocking(
            crate::tools::Tool::Ffmpeg,
            &crate::tools::ToolContext::default(),
        );
        ffmpeg.available.then_some(ffmpeg.path)
    }

    /// `ffmpeg -i <file>`: what it prints about the input.
    pub fn probe(path: &Path) -> Option<Probe> {
        let out = Command::new(binary()?)
            .args(["-hide_banner", "-nostdin", "-i"])
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .output()
            .ok()?;
        Some(parse_probe(&String::from_utf8_lossy(&out.stderr)))
    }

    /// The first video stream (the attached picture) as stored.
    pub fn picture(path: &Path) -> Option<EmbeddedPicture> {
        let out = Command::new(binary()?)
            .args(["-hide_banner", "-nostdin", "-v", "error", "-i"])
            .arg(path)
            .args([
                "-map",
                "0:v:0",
                "-c",
                "copy",
                "-frames:v",
                "1",
                "-f",
                "image2pipe",
                "pipe:1",
            ])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        if !out.status.success() || out.stdout.is_empty() {
            return None;
        }
        Some(EmbeddedPicture {
            data: out.stdout,
            front: true,
        })
    }

    fn parse_duration(text: &str) -> i64 {
        let Some(raw) = text
            .split("Duration: ")
            .nth(1)
            .and_then(|s| s.split(',').next())
            .map(str::trim)
        else {
            return 0;
        };
        let mut parts = raw.split(':');
        let mut next = || parts.next().and_then(|p| p.parse::<f64>().ok());
        match (next(), next(), next()) {
            (Some(h), Some(m), Some(s)) => {
                let ms = ((h * 3600.0 + m * 60.0 + s) * 1000.0).round() as i64;
                ms.max(0)
            }
            _ => 0,
        }
    }

    /// `"3/12"` → (3, 12).
    fn number_pair(raw: &str) -> (Option<i64>, Option<i64>) {
        let mut it = raw.split('/');
        let n = |s: Option<&str>| {
            s.and_then(|v| v.trim().parse::<i64>().ok())
                .filter(|n| *n > 0)
        };
        (n(it.next()), n(it.next()))
    }

    /// Duration, the input's own metadata block (not the streams') and
    /// whether a picture stream is there.
    pub(crate) fn parse_probe(text: &str) -> Probe {
        let mut tags = EmbeddedTags::default();
        let mut genres: Vec<String> = Vec::new();
        let mut in_meta = false;
        let mut last_key = String::new();
        let mut dates: Vec<String> = Vec::new();
        for line in text.lines() {
            let indent = line.len() - line.trim_start().len();
            let trimmed = line.trim();
            if trimmed == "Metadata:" && indent == 2 {
                in_meta = true;
                continue;
            }
            if !in_meta {
                continue;
            }
            if indent < 4 {
                in_meta = false;
                continue;
            }
            let Some((k, v)) = trimmed.split_once(':') else {
                continue;
            };
            let key = k.trim().to_ascii_lowercase();
            let value = v.trim();
            if key.is_empty() {
                // Continuation of a multi-line value (lyrics, comments).
                if last_key == "lyrics" {
                    if let Some(l) = tags.lyrics.as_mut() {
                        l.push('\n');
                        l.push_str(value);
                    }
                }
                continue;
            }
            last_key.clear();
            let Some(value) = clean(value) else {
                continue;
            };
            match key.as_str() {
                "title" => tags.title = Some(value),
                "artist" | "author" => tags.artist = Some(value),
                "album_artist" | "album artist" | "albumartist" | "wm/albumartist" => {
                    tags.album_artist = Some(value)
                }
                "album" | "wm/albumtitle" => tags.album = Some(value),
                "track" | "tracknumber" | "wm/tracknumber" => {
                    let (n, total) = number_pair(&value);
                    tags.track_number = n;
                    tags.track_total = tags.track_total.or(total);
                }
                "tracktotal" | "totaltracks" => {
                    tags.track_total = number_pair(&value).0;
                }
                "disc" | "discnumber" | "wm/partofset" => {
                    let (n, total) = number_pair(&value);
                    tags.disc_number = n;
                    tags.disc_total = tags.disc_total.or(total);
                }
                "disctotal" | "totaldiscs" => tags.disc_total = number_pair(&value).0,
                "date" | "year" | "wm/year" | "originaldate" | "creation_time_tag" => {
                    if let Some(d) = normalize_date(&value) {
                        dates.push(d);
                    }
                }
                "genre" | "wm/genre" => genres.push(value),
                "bpm" | "tbpm" | "tempo" | "wm/beatsperminute" => tags.bpm = parse_bpm(&value),
                "musicbrainz_trackid" | "musicbrainz track id" | "musicbrainz/track id" => {
                    tags.mb_recording_id = Some(value)
                }
                "musicbrainz_albumid" | "musicbrainz album id" | "musicbrainz/album id" => {
                    tags.mb_release_id = Some(value)
                }
                "musicbrainz_artistid" | "musicbrainz artist id" | "musicbrainz/artist id" => {
                    tags.mb_artist_id = Some(value)
                }
                "musicbrainz_releasegroupid"
                | "musicbrainz release group id"
                | "musicbrainz/release group id" => tags.mb_release_group_id = Some(value),
                k if k == "lyrics" || k.starts_with("lyrics-") || k == "unsyncedlyrics" => {
                    tags.lyrics = Some(value);
                    last_key = "lyrics".into();
                }
                _ => {}
            }
        }
        if !genres.is_empty() {
            tags.genre = Some(genres.join("; "));
        }
        tags.date = dates.into_iter().fold(None, |best: Option<String>, d| {
            if best
                .as_deref()
                .is_none_or(|b| date_precision(&d) > date_precision(b))
            {
                Some(d)
            } else {
                best
            }
        });
        let has_picture = text
            .lines()
            .any(|l| l.trim_start().starts_with("Stream #") && l.contains(": Video:"));
        Probe {
            duration_ms: parse_duration(text),
            tags,
            has_picture,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parses_global_metadata_only() {
            let text = "Input #0, matroska,webm, from 'x.webm':\n  Metadata:\n    title           : Web Title\n    track           : 3/9\n    ARTIST          : Web Artist\n    GENRE           : Synthwave;Electronic\n    DATE            : 2021-03-04\n    LYRICS          : line one\n                    : line two\n  Duration: 00:01:02.50, start: 0.000000, bitrate: 30 kb/s\n  Stream #0:0: Audio: opus, 48000 Hz, mono, fltp\n    Metadata:\n      title           : Stream title\n  Stream #0:1: Video: mjpeg, yuvj420p, 16x16 (attached pic)\n";
            let p = parse_probe(text);
            assert_eq!(p.duration_ms, 62_500);
            assert!(p.has_picture);
            assert_eq!(p.tags.title.as_deref(), Some("Web Title"));
            assert_eq!(p.tags.artist.as_deref(), Some("Web Artist"));
            assert_eq!(p.tags.track_number, Some(3));
            assert_eq!(p.tags.track_total, Some(9));
            assert_eq!(p.tags.genre.as_deref(), Some("Synthwave;Electronic"));
            assert_eq!(p.tags.date.as_deref(), Some("2021-03-04"));
            assert_eq!(p.tags.lyrics.as_deref(), Some("line one\nline two"));
        }
    }
}
