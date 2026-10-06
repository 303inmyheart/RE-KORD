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
Embedded tags fill in and refine what the folders say:

| Field | Tag source | Notes |
|---|---|---|
| Title | title | When missing, or only the file name, the file name is cleaned: no `01 - ` prefix, no `[…]`, `(Official Video)` or `(Remaster)` noise. Musical versions such as `(Remix)`, `(Live)` and `(feat. …)` are kept. |
| Artist | artist | Taken from the folder by default. Taken from the tag in the flat and tag-based layouts. |
| Album | album | The album name is the curated title, else the most common album tag, else the folder name. |
| Genre | genre | Multiple genres are split on `;` `/` `,` `|`, normalised and deduplicated (for example `["Hip Hop", "Pop Rap"]`). |
| Date | recording date, release date, original release date, year | Kept as precise as the source: `YYYY-MM-DD`, `YYYY-MM` or `YYYY`. |
| Track / disc number | track, disc | Falls back to the file name (`07 - x`, `1-07 x`). |
| BPM | `TBPM` (ID3), `tmpo` (MP4), `BPM` (Vorbis/APE), `TEMPO` | Used by Sonic Nebula. |
| Lyrics | lyrics (`USLT` and equivalents) | Synced LRC or plain text. |

Values a person curated (in Studio, from a metadata fetch, from a legacy library) always win
over tags, and rescans never overwrite them.

## Cover art

Album covers are files in the album folder. The hub looks for `cover.jpg`, `folder.jpg`,
`front.jpg`, `cover.png`, `folder.png` and `artwork.jpg`, case-insensitively. Studio writes
the cover you pick or upload there. Grids load 128 px and 256 px thumbnails that the hub
generates and caches in `<data dir>/thumbs/`.

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
