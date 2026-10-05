//! Track / album matching thresholds against recorded-shape provider
//! payloads (no network).

use rekord_core::metadata::discogs::DiscogsTrackEntry;
use rekord_core::metadata::matching::{
    audiodb_track_candidates, deezer_candidates, itunes_track_candidates, match_tracklist,
    musicbrainz_tracklist, pick_best_track, sanitize_expected_track_count, score_album_candidate,
    AlbumQuery, TrackQuery,
};
use rekord_core::metadata::prepare_track_title_for_meta;
use serde_json::json;

fn desperado_query(file_stem: &str, n: i64) -> TrackQuery {
    let title = prepare_track_title_for_meta("Eagles", file_stem);
    TrackQuery {
        artist: "Eagles".into(),
        album: "Desperado".into(),
        title: title.clone(),
        title_from_file: title,
        duration_ms: None,
        track_number: Some(n),
        disc_number: None,
    }
}

/// QA studio #3: Deezer answers "Twenty-One" with Neil Young's "Heart of
/// Gold"; next used to take the first row.
#[test]
fn eagles_twenty_one_rejects_heart_of_gold() {
    let deezer = json!({"data": [{
        "id": 3135553, "title": "Heart of Gold", "duration": 187,
        "track_position": 7, "disk_number": 1,
        "link": "https://www.deezer.com/track/3135553",
        "artist": {"name": "Neil Young"},
        "album": {"title": "Harvest (2009 Remaster)"}
    }], "total": 1});
    let q = desperado_query("03 - Twenty-One", 3);
    assert!(pick_best_track(&q, &deezer_candidates(&deezer)).is_none());
}

#[test]
fn same_artist_but_other_song_is_rejected_too() {
    let deezer = json!({"data": [
        {"id": 1, "title": "Hotel California", "artist": {"name": "Eagles"}, "album": {"title": "Hotel California"}},
        {"id": 2, "title": "Take It Easy", "artist": {"name": "Eagles"}, "album": {"title": "Eagles"}}
    ]});
    let q = desperado_query("03 - Twenty-One", 3);
    assert!(pick_best_track(&q, &deezer_candidates(&deezer)).is_none());
}

#[test]
fn compilation_hit_gives_no_track_number() {
    // Right song, other release: accepted, but not "same release".
    let itunes = json!({"resultCount": 1, "results": [{
        "wrapperType": "track", "kind": "song", "trackId": 9,
        "artistName": "Eagles", "collectionName": "Their Greatest Hits 1971-1975",
        "trackName": "Desperado", "trackNumber": 4, "discNumber": 1, "trackTimeMillis": 213000,
        "primaryGenreName": "Rock", "releaseDate": "1976-02-17T08:00:00Z"
    }]});
    let q = desperado_query("06 - Desperado", 6);
    let best = pick_best_track(&q, &itunes_track_candidates(&itunes)).unwrap();
    assert_eq!(best.candidate.title, "Desperado");
    assert!(
        !best.album_matches,
        "a compilation is not the Desperado album"
    );
}

#[test]
fn tracklist_first_by_title_with_position_support() {
    // MusicBrainz release with recordings (shape of /ws/2/release/{id}?inc=recordings).
    let mb = json!({"id": "x", "title": "Desperado", "media": [{"position": 1, "track-count": 4, "tracks": [
        {"position": 1, "title": "Doolin-Dalton", "length": 206000},
        {"position": 2, "title": "Twenty-One", "length": 131000},
        {"position": 3, "title": "Out of Control", "length": 184000},
        {"position": 4, "title": "Tequila Sunrise", "length": 172000}
    ]}]});
    let list = musicbrainz_tracklist(&mb);
    assert_eq!(list.len(), 4);
    // File numbered 03 but titled Twenty-One: the title wins.
    let m = match_tracklist(&desperado_query("03 - Twenty-One", 3), &list).unwrap();
    assert_eq!(m.entry.position, Some(2));
    assert!(!m.position_matches);
    // Unknown title: the position alone never matches.
    assert!(match_tracklist(&desperado_query("03 - Track 3", 3), &list).is_none());
    // Duration far off (a 9-minute live version) is rejected.
    let mut q = desperado_query("04 - Tequila Sunrise", 4);
    q.duration_ms = Some(540_000);
    assert!(match_tracklist(&q, &list).is_none());
}

#[test]
fn audiodb_hits_need_artist_and_title() {
    let adb = json!({"track": [{
        "idTrack": "1", "strTrack": "Desperado", "strArtist": "Rihanna",
        "strAlbum": "Anti", "intTrackNumber": "4",
        "strDescriptionEN": "A long description that must never become lyrics."
    }]});
    let q = desperado_query("06 - Desperado", 6);
    assert!(pick_best_track(&q, &audiodb_track_candidates(&adb)).is_none());
}

#[test]
fn album_sanity_for_singles_and_counts() {
    let q = AlbumQuery {
        artist: "Eagles".into(),
        album: "Desperado".into(),
        local_track_count: 11,
    };
    assert!(score_album_candidate(&q, "Eagles", "Desperado - Single", Some(1)).is_none());
    assert!(score_album_candidate(&q, "Linda Ronstadt", "Desperado", Some(11)).is_none());
    let ok = score_album_candidate(&q, "Eagles", "Desperado (2013 Remaster)", Some(11)).unwrap();
    assert!(ok >= 0.85, "{ok}");
    assert_eq!(sanitize_expected_track_count(Some(1), 8), None);
    assert_eq!(sanitize_expected_track_count(Some(11), 11), Some(11));
    let _ = DiscogsTrackEntry {
        disc: 1,
        position: Some(1),
        title: "x".into(),
        duration_ms: None,
    };
}
