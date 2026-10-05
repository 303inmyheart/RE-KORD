//! Title cleaning: ports of legacy `server/sanitizeLocalTrackTitle.test.ts`
//! plus `prepareTrackTitleForMeta` / `cleanTrackTitleForSearch` cases.

use rekord_core::metadata::sanitize_local_track_title_display as sanitize;
use rekord_core::metadata::text::{
    clean_album_name_for_search, clean_track_title_for_search, prepare_track_title_for_meta,
};

fn s(raw: &str, artist_folder: Option<&str>) -> String {
    sanitize(raw, artist_folder, None)
}

#[test]
fn removes_brackets_and_numbering() {
    assert_eq!(s("01 - Foo [2024] Bar", None), "Foo Bar");
    assert_eq!(s("3. Song", None), "Song");
}

#[test]
fn removes_official_video_suffixes() {
    assert_eq!(s("Song (Official Music Video)", None), "Song");
    assert_eq!(s("Track (lyrics)", None), "Track");
}

#[test]
fn removes_topic_and_redundant_artist_prefix() {
    assert_eq!(s("Luna (Official Audio) - Topic", Some("NotUsed")), "Luna");
    assert_eq!(
        s(
            "Måneskin - Zitti e buoni (Official Video)",
            Some("Måneskin")
        ),
        "Zitti e buoni"
    );
}

#[test]
fn keeps_prefix_of_another_artist() {
    let t = "Altra banda - Un brano";
    assert_eq!(s(t, Some("Måneskin")), t);
}

#[test]
fn removes_trailing_artist_and_keeps_feat() {
    let base = "02 - Good Goodbye [Official Music Video] - Linkin Park (feat. Pusha T and Stormzy)";
    assert_eq!(
        s(base, Some("Linkin Park")),
        "Good Goodbye (feat. Pusha T and Stormzy)"
    );
    assert_eq!(
        s(
            "01 - Nobody Can Save Me (Official Audio) - Linkin Park",
            Some("Linkin Park")
        ),
        "Nobody Can Save Me"
    );
}

#[test]
fn removes_original_and_remaster_parens_keeps_feat() {
    assert_eq!(s("Brano (Original Mix) (2017 remaster)", None), "Brano");
    assert_eq!(s("X (official audio) (feat. Y)", None), "X (feat. Y)");
}

#[test]
fn track_artist_from_sidecar_wins_over_folder() {
    assert_eq!(
        sanitize("Jay-Z - 99 Problems", Some("Various"), Some("Jay-Z")),
        "99 Problems"
    );
}

/// QA library #15: next stripped versions and words inside words.
#[test]
fn keeps_remix_live_and_inner_words() {
    for keep in [
        "Titanium (Robin Schulz Remix)",
        "Song (Live Edit)",
        "Tree (Olive Tree)",
        "Name (Videodrome)",
        "Track (Remixed)",
        "In the Evening",
        "Livin' on a Prayer",
        "Instrumentals Are Fun",
    ] {
        assert_eq!(s(keep, None), keep, "{keep}");
    }
    // Packaging cruft still goes.
    assert_eq!(s("Song (HD)", None), "Song");
    assert_eq!(s("Song (Visualizer)", None), "Song");
    assert_eq!(s("Song (2011 Remastered)", None), "Song");
}

#[test]
fn search_cleaning_like_legacy() {
    assert_eq!(
        clean_track_title_for_search("Måneskin - Zitti e buoni (Official Video)", "Måneskin"),
        "Zitti e buoni"
    );
    assert_eq!(
        clean_track_title_for_search("01. Twenty-One", ""),
        "Twenty-One"
    );
    assert_eq!(clean_track_title_for_search("Song | Lyrics", ""), "Song");
    assert_eq!(clean_track_title_for_search("Song // Live", ""), "Song");
    assert_eq!(
        clean_track_title_for_search("Who? (Official Audio)", ""),
        "Who"
    );
    assert_eq!(clean_track_title_for_search("Track [HD]", ""), "Track");
    assert_eq!(
        clean_track_title_for_search("Intro [Skit]", ""),
        "Intro [Skit]"
    );
    assert_eq!(
        clean_track_title_for_search("Song - Official Music Video", ""),
        "Song"
    );
    assert_eq!(clean_track_title_for_search("Tom & Jerry", ""), "Tom Jerry");
    assert_eq!(
        clean_track_title_for_search("Eagles: Desperado (Remastered)", "Eagles"),
        "Desperado"
    );
}

#[test]
fn prepare_falls_back_to_the_raw_title() {
    assert_eq!(
        prepare_track_title_for_meta("Eagles", "03 - Twenty-One"),
        "Twenty-One"
    );
    // Everything would be stripped: keep the original.
    assert_eq!(prepare_track_title_for_meta("", "[HD]"), "[HD]");
    assert_eq!(clean_album_name_for_search("？!  Deluxe"), "! Deluxe");
}
