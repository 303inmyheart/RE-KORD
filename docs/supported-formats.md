# Supported formats

What RE-KORD indexes, what it plays, what it converts and which tags it reads.

## Audio files

| Extension | Indexed | Played | Notes |
|---|:-:|:-:|---|
| `.mp3` | yes | directly | Long VBR files get an exact duration (see below) |
| `.m4a`, `.aac` | yes | directly | Served as `audio/mp4` |
| `.flac` | yes | directly | Transcoded to MP3 when cast |
| `.ogg` | yes | directly | Transcoded to MP3 when cast |
| `.opus` | yes | directly | Transcoded to MP3 when cast |
| `.wav` | yes | directly | Transcoded to MP3 when cast |
| `.webm` | yes | directly | Audio-only WebM, as written by yt-dlp |
| `.wma` | yes | via FLAC copy | Converted by the hub |
| `.aiff`, `.aif` | yes | via FLAC copy | Converted by the hub |
| `.alac` | yes | via FLAC copy | Converted by the hub |

The list of indexed extensions is `AUDIO_EXT` in
[`crates/core/src/layout.rs`](../crates/core/src/layout.rs).

### Formats the player cannot decode

WebKitGTK (Linux), the Android WebView and Chromium cannot decode WMA, AIFF or raw ALAC.
For these files the client asks the hub for
`/api/v1/transcode/<path>?format=flac`. The hub converts the file once into a lossless FLAC
copy and serves that copy like any other library file, with HTTP Range support. Seeking,
duration and resume all work.

- The client only takes this path when its engine cannot play the file
  (`canPlayType`), or when a direct play fails with "format not supported". An engine that
  can decode the original gets the original.
- Copies are cached in `<data dir>/cache/transcode`. The cache is capped at 2 GiB and
  evicts the least recently used files first.
- This needs **ffmpeg** on the hub. The Server packages bundle it; Docker installs it. With
  the headless or source build, install it from your distribution if `bin/ffmpeg` is
  missing. `GET /api/v1/health` reports `"transcode": true` when ffmpeg is usable.

On Linux the desktop apps decode audio through GStreamer. The AppImage bundles only the
plugins playback needs (MP3, AAC/M4A/ALAC, FLAC, Ogg Vorbis, Opus, WAV, WebM/Matroska and the
audio output) and caches GStreamer's plugin registry in `~/.cache/re-kord/`, so it starts
without rescanning; the `.deb` packages depend on `gstreamer1.0-plugins-base`,
`gstreamer1.0-plugins-good` and `gstreamer1.0-libav` (the last one is needed for AAC/M4A).

### Cast

Google Cast receivers (Chromecast, Google Home, Nest) often fail on FLAC, OGG, Opus and
WAV. Those four are sent through a live MP3 transcode (320 kbps) when the hub has ffmpeg.
Everything else is streamed as stored. See
[`apps/client-ui/src/lib/cast/castMedia.ts`](../apps/client-ui/src/lib/cast/castMedia.ts).

## Long VBR MP3 files

Variable-bitrate MP3s without a Xing/VBRI header (long DJ sets and many YouTube rips) make
most players guess the duration from the first frames, so the duration and seek bar are
wrong. RE-KORD counts the frames during the scan, stores the exact duration, and builds a
synthetic Xing header. `/media` splices that header in front of the first frame when it
serves the file, so the browser sees the correct duration and seeks accurately. When a
container cannot be timed from its tags (WMA/ASF, WebM), the scan asks ffmpeg.

## Tags

The library is organised by folders first (see [Library layout](#library-layout)).
Embedded tags fill in and refine what the folders say. They are read on the first scan of a
file and again whenever the file changes (size or modification time), in the same read
that times the file. The admin panel switch **Library › Embedded metadata › "Read embedded
metadata and covers"** (on by default) turns this off; the file and folder names are then
the only source.

### Where the tags come from

| Format | Tags read | Embedded picture |
|---|---|---|
| `.mp3` | ID3v2 (2.2–2.4), then ID3v1 and APE for what ID3v2 lacks | ID3v2 `APIC` |
| `.flac` | Vorbis comments, plus an ID3v2 block in front of the stream | `PICTURE` blocks |
| `.ogg`, `.opus` | Vorbis comments | `METADATA_BLOCK_PICTURE` |
| `.m4a`, `.aac` (MP4), `.alac` | MP4 `ilst` atoms (`©nam`, `©ART`, `aART`, `trkn`, `disk`, `tmpo`, `©lyr`, iTunes freeform) | `covr` |
| `.aac` (ADTS) | ID3v2 / ID3v1 | ID3v2 `APIC` |
| `.wav` | ID3v2 chunk, then RIFF `INFO` | ID3v2 `APIC` |
| `.aiff`, `.aif` | ID3v2 chunk, then AIFF text chunks | ID3v2 `APIC` |
| `.wma` | ASF metadata, through `ffmpeg -i` (needs ffmpeg) | `WM/Picture` (ffmpeg) |
| `.webm` | Matroska tags, through `ffmpeg -i` (needs ffmpeg) | attachments ffmpeg exposes as a picture stream |

The format is recognised from the file's content (a `.m4a` that is really WebM still
works); the extension is only the fallback. Every tag in a file is read and merged: the
format's main tag first, the others for what it lacks.

### Fields

| Field | Tag source | Notes |
|---|---|---|
| Title | title | When missing, or only the file name, the file name is cleaned: no `01 - ` prefix, no `[…]`, `(Official Video)` or `(Remaster)` noise. Musical versions such as `(Remix)`, `(Live)` and `(feat. …)` are kept. |
| Artist | artist (all values, joined with `, `) | Shown as `track_artist`. The library artist is the folder by default; taken from the tag in the flat and tag-based layouts. |
| Album artist | album artist | `album_artist` on tracks and albums. |
| Album | album | The album name is the curated title, else the most common album tag, else the folder name. |
| Genre | genre (every value) | Multiple values and values separated by `;` `/` `,` `|` are split, normalised and deduplicated (for example `["Hip Hop", "Pop Rap"]`). |
| Date | recording date, release date, original release date, year | Kept as precise as the source: `YYYY-MM-DD`, `YYYY-MM` or `YYYY`. |
| Track / disc number and totals | track, disc, track total, disc total; `4/11` forms | Numbers fall back to the file name (`07 - x`, `1-07 x`). |
| BPM | `TBPM` (ID3), `tmpo` (MP4), `BPM` (Vorbis/APE), `TEMPO` | Used by Sonic Nebula. |
| Lyrics | `USLT`, `©lyr`, `LYRICS`, `UNSYNCEDLYRICS` | LRC text shows as synced lyrics. ID3 `SYLT` (binary synced lyrics) is not read. |
| MusicBrainz ids | recording, release, artist, release group | `musicbrainz` on tracks; the release id also fills the album's `musicbrainz_release_id` when empty. |

### Precedence

Per field, the first value found wins:

1. a value typed by a person in Studio (always);
2. with the default priority **"Studio > embedded > file name"**: a curated value (Studio,
   `kord-albuminfo.json` / `kord-trackinfo.json` sidecars, a metadata fetch, the legacy
   library);
3. the file's embedded tags;
4. the file and folder names.

With the priority **"Embedded > Studio (filling only)"** the tags also replace curated
values nobody typed (step 3 before step 2). Typed values are replaced only by the admin's
explicit **"Re-read and replace Studio values too"**. The API reports where values come
from: `curated_fields` and `embedded_fields` on tracks and albums.

A value that came from the tags follows the file: when the tag changes the value changes,
when the tag is removed the file-name fallback (or nothing) takes its place. Lyrics stored
before 5.1 count as curated (they may come from LRCLIB) until a read finds the very same
text in the file.

Libraries indexed by 5.0 are completed by a background job, *Reading embedded metadata*
(admin panel **Jobs**): about 30 files per batch, tags only (no pictures, no durations), a
pause between batches and none at all while a scan runs. Progress is stored per track, so a
restart or a cancel resumes where it stopped. The same job runs after **Maintenance ›
"Re-read embedded metadata"** or a change of the embedded settings.

## Cover art

An album's cover is, in this order:

1. an image in the album folder: `cover.jpg`, `folder.jpg`, `front.jpg`, `cover.png`,
   `folder.png` or `artwork.jpg`, case-insensitively. Covers chosen in Studio (searched or
   uploaded) are written there as `cover.jpg`;
2. a cover registered by the legacy app (`.kord/artwork`);
3. the picture embedded in one of the album's files: the front cover of the first track
   that has one (at most three tracks are opened), else the first picture found.

Embedded pictures are stored in `<data dir>/covers/embedded/` (one JPEG per album; a
JPEG of up to 1500 px is kept as it is, anything larger or in PNG / WebP is scaled to 1500 px
at most and stored as JPEG). The scan never writes into the music folder: it may be
read-only or on a NAS, and new files there would wake the folder watcher. A picture that
cannot be decoded is skipped and logged once; the album is looked at again only when its
files change. Stored covers that no album uses any more are deleted after each scan.
Loose tracks (the virtual **Tracks** album) never get an embedded cover.

The API reports the origin as `cover_source` (`folder`, `legacy`, `embedded`). Grids load
128 px and 256 px thumbnails that the hub generates and caches in `<data dir>/thumbs/`,
whatever the origin; `cover_version` changes when the cover does.

## Library layout

RE-KORD expects `Music/Artist/Album/track` by default and detects other layouts when you
choose the music folder:

| Layout | Shape | Notes |
|---|---|---|
| Artist / Album / Track | `Artist/Album/01 - Title.flac` | The default |
| Artist / Track | `Artist/Title.mp3` | Loose tracks go into a virtual album called **Tracks** |
| Flat | `Title.mp3` in the root | The artist comes from the tags |
| Tags | any | ID3 and other tags win over folder names |

The layout is stored in `<music folder>/.kord/library-layout.json`, so it travels with the
library. Hidden folders, RE-KORD's own folders (`.kord`, `.rekord`) and NAS housekeeping
folders (`@eaDir`, `#recycle`) are never indexed.
