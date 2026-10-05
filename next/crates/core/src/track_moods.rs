//! Track mood ids (parity legacy `server/trackMoods.mjs`).
//!
//! Moods are personal (user-state `trackMoods`): at most three per track, from
//! a fixed list, order preserved, unique. `uplifting_happy` is the old id of
//! `motivational_drive`.

use serde_json::{Map, Value};

pub const TRACK_MOOD_IDS: &[&str] = &[
    "energy_boost",
    "party_dance",
    "chill_relax",
    "focus_study",
    "romantic_intimacy",
    "sad_melancholy",
    "dark_tense",
    "aggressive_heavy",
    "dreamy_ethereal",
    "epic_cinematic",
    "nostalgia_retro",
    "fun_quirky",
    "soulful_groovy",
    "motivational_drive",
];

pub const MAX_TRACK_MOODS: usize = 3;

/// Canonical id for one raw mood value, or `None` when not allowed.
pub fn normalize_track_mood(raw: &str) -> Option<&'static str> {
    let s = raw.trim();
    let s = if s == "uplifting_happy" {
        "motivational_drive"
    } else {
        s
    };
    TRACK_MOOD_IDS.iter().copied().find(|id| *id == s)
}

/// Normalise a mood list (`moods` array) plus the old single `mood` field.
pub fn normalize_track_moods_list(
    primary: Option<&Value>,
    legacy_mood: Option<&Value>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |raw: &Value| {
        if out.len() >= MAX_TRACK_MOODS {
            return;
        }
        if let Some(id) = raw.as_str().and_then(normalize_track_mood) {
            if !out.iter().any(|x| x == id) {
                out.push(id.to_string());
            }
        }
    };
    match primary {
        Some(Value::Array(items)) => items.iter().for_each(&mut add),
        // A bare string is accepted as a one-item list.
        Some(v @ Value::String(_)) => add(v),
        _ => {}
    }
    if let Some(v) = legacy_mood {
        add(v);
    }
    out
}

/// Normalise a whole `trackMoods` map: invalid ids dropped, empty entries removed.
pub fn normalize_track_moods_map(map: Map<String, Value>) -> Map<String, Value> {
    let mut out = Map::new();
    for (rel, moods) in map {
        let key = rel.trim();
        if key.is_empty() {
            continue;
        }
        let list = match &moods {
            Value::Object(obj) => normalize_track_moods_list(obj.get("moods"), obj.get("mood")),
            other => normalize_track_moods_list(Some(other), None),
        };
        if !list.is_empty() {
            out.insert(
                key.to_string(),
                Value::Array(list.into_iter().map(Value::String).collect()),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keeps_order_dedups_and_caps_at_three() {
        let v = json!([
            "chill_relax",
            "bogus",
            "chill_relax",
            "dark_tense",
            "fun_quirky",
            "party_dance"
        ]);
        assert_eq!(
            normalize_track_moods_list(Some(&v), None),
            vec!["chill_relax", "dark_tense", "fun_quirky"]
        );
    }

    #[test]
    fn maps_uplifting_alias_and_legacy_single_field() {
        let v = json!([]);
        assert_eq!(
            normalize_track_moods_list(Some(&v), Some(&json!("uplifting_happy"))),
            vec!["motivational_drive"]
        );
    }

    #[test]
    fn map_drops_empty_and_invalid_entries() {
        let m = json!({
            "A/B/01.mp3": ["energy_boost"],
            "A/B/02.mp3": ["nope"],
            "": ["energy_boost"],
            "A/B/03.mp3": { "moods": ["focus_study"], "mood": "sad_melancholy" }
        });
        let out = normalize_track_moods_map(m.as_object().unwrap().clone());
        assert_eq!(out.len(), 2);
        assert_eq!(out["A/B/03.mp3"], json!(["focus_study", "sad_melancholy"]));
    }
}
