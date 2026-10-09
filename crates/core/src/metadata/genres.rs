//! Genre lists: split, clean and canonicalize (legacy `server/genres.mjs`
//! `parseTrackGenres` / `normalizeStoredGenreString`, plus per-token junk
//! removal and one canonical label per spelling: "Hip Hop" for
//! "hip-hop" / "Hip-Hop" / "hiphop"). Tokens are split with the DB's
//! `split_genres` so Studio and the library genre index agree.
//!
//! Stored form: tokens joined with `"; "`.

use super::text::normalize_name;
use std::collections::HashSet;

/// Normalized comparison key of one genre token: lowercase, accents folded,
/// punctuation and spaces dropped ("Hip-Hop" and "hip hop" → "hiphop").
pub fn genre_key(token: &str) -> String {
    normalize_name(token).replace(' ', "")
}

/// Canonical display labels for spellings that vary between providers.
const CANONICAL: &[(&str, &str)] = &[
    ("hiphop", "Hip Hop"),
    ("rap", "Rap"),
    ("rnb", "R&B"),
    ("randb", "R&B"),
    ("rhythmandblues", "R&B"),
    // `&` is dropped by the key: "R&B", "Rhythm & Blues", "Drum & Bass",
    // "Rock & Roll" (the client's table has the same aliases).
    ("rb", "R&B"),
    ("rhythmblues", "R&B"),
    ("poprock", "Pop Rock"),
    ("electronic", "Electronic"),
    ("electronica", "Electronica"),
    ("edm", "EDM"),
    ("dancepop", "Dance Pop"),
    ("synthpop", "Synth-pop"),
    ("triphop", "Trip Hop"),
    ("lofi", "Lo-Fi"),
    ("lofihiphop", "Lo-Fi Hip Hop"),
    ("drumandbass", "Drum and Bass"),
    ("drumnbass", "Drum and Bass"),
    ("drumbass", "Drum and Bass"),
    ("dnb", "Drum and Bass"),
    ("kpop", "K-Pop"),
    ("jpop", "J-Pop"),
    ("altrock", "Alternative Rock"),
    ("alternativerock", "Alternative Rock"),
    ("alternative", "Alternative"),
    ("indierock", "Indie Rock"),
    ("indiepop", "Indie Pop"),
    ("hardrock", "Hard Rock"),
    ("heavymetal", "Heavy Metal"),
    ("numetal", "Nu Metal"),
    ("punkrock", "Punk Rock"),
    ("poppunk", "Pop Punk"),
    ("rocknroll", "Rock & Roll"),
    ("rockandroll", "Rock & Roll"),
    ("rockroll", "Rock & Roll"),
    ("rock", "Rock"),
    ("pop", "Pop"),
    ("soul", "Soul"),
    ("funk", "Funk"),
    ("jazz", "Jazz"),
    ("blues", "Blues"),
    ("metal", "Metal"),
    ("punk", "Punk"),
    ("reggae", "Reggae"),
    ("reggaeton", "Reggaeton"),
    ("trap", "Trap"),
    ("house", "House"),
    ("techno", "Techno"),
    ("trance", "Trance"),
    ("dubstep", "Dubstep"),
    ("ambient", "Ambient"),
    ("classical", "Classical"),
    ("country", "Country"),
    ("folk", "Folk"),
    ("soundtrack", "Soundtrack"),
    ("singersongwriter", "Singer-Songwriter"),
    ("cantautorato", "Cantautorato"),
    ("italianpop", "Italian Pop"),
    ("popitaliano", "Pop italiano"),
    ("rapitaliano", "Rap italiano"),
    ("hiphopitaliano", "Hip Hop italiano"),
    ("worldwide", "Worldwide"),
];

/// Canonical label for a token (a known spelling, else the token trimmed
/// with whitespace squashed).
pub fn canonical_genre_label(token: &str) -> String {
    let key = genre_key(token);
    if let Some((_, label)) = CANONICAL.iter().find(|(k, _)| *k == key) {
        return (*label).to_string();
    }
    token.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Legacy `parseTrackGenres`: split on `;`, then `/` and `,` (unique,
/// case-insensitive, in order). No cleaning.
pub fn parse_track_genres(raw: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for seg in raw.split(';') {
        for p in seg.split(['/', ',']) {
            let x = p.trim();
            if x.is_empty() || !seen.insert(x.to_lowercase()) {
                continue;
            }
            out.push(x.to_string());
        }
    }
    out
}

/// Split, drop junk tokens (numbers, ID3v1 `(17)`, stubs like "Music"),
/// canonicalize and de-duplicate. Splitting and the duplicate key are the
/// DB's ([`crate::db::text::split_genres`] / [`crate::db::text::genre_key`])
/// so the library genre index and Studio agree.
pub fn clean_genre_tokens(raw: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for token in crate::db::text::split_genres(raw) {
        let label = canonical_genre_label(&token);
        if seen.insert(crate::db::text::genre_key(&label)) {
            out.push(label);
        }
    }
    out
}

/// Stored genre string (`"Rock; Hip Hop"`) or `None` when nothing is left.
pub fn normalize_genre_string(raw: &str) -> Option<String> {
    let parts = clean_genre_tokens(raw);
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// [`normalize_genre_string`] for an optional value.
pub fn normalize_genre_opt(raw: Option<&str>) -> Option<String> {
    raw.and_then(normalize_genre_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ampersand_aliases_share_one_label() {
        for (a, b) in [
            ("Drum & Bass", "Drum and Bass"),
            ("Rhythm & Blues", "R&B"),
            ("R&B", "rnb"),
            ("Rock & Roll", "Rock and Roll"),
        ] {
            assert_eq!(
                canonical_genre_label(a),
                canonical_genre_label(b),
                "{a} / {b}"
            );
        }
        assert_eq!(canonical_genre_label("Drum & Bass"), "Drum and Bass");
        assert_eq!(canonical_genre_label("R&B"), "R&B");
    }

    #[test]
    fn variants_collapse_to_one_label() {
        assert_eq!(
            normalize_genre_string("Hip Hop; hip-hop, Hip-Hop / Pop Rap").as_deref(),
            Some("Hip Hop; Pop Rap")
        );
    }

    #[test]
    fn numeric_and_stub_tokens_are_dropped() {
        assert_eq!(
            normalize_genre_string("Rock; 8; Music").as_deref(),
            Some("Rock")
        );
        assert_eq!(normalize_genre_string("(17)"), None);
        assert_eq!(normalize_genre_string("  "), None);
    }

    #[test]
    fn legacy_split() {
        assert_eq!(
            normalize_genre_string("rnb; Hip-Hop/Rap").as_deref(),
            Some("R&B; Hip Hop; Rap")
        );
        assert_eq!(
            parse_track_genres("Rock / Pop, Rock; Jazz"),
            vec!["Rock", "Pop", "Jazz"]
        );
    }
}
