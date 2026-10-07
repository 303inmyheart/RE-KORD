/**
 * "Playlist al volo": conteggi dal vivo, chip visibili, copertine, selezione salvata.
 * Si lancia con `pnpm test`.
 */
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  computeMixFacets,
  computeMixTotals,
  parseMixSelection,
  pickCoverTracks,
  pruneMixSelection,
  sortGenreKeys,
  visibleGenreKeys,
  visibleMoodIds,
} from "./mixFilter.ts";
import { NO_GENRE_KEY, TRACK_MOOD_IDS } from "./trackMoods.ts";

const NONE = NO_GENRE_KEY;
const entries = [
  { genres: ["hiphop"], moods: ["dark_tense", "energy_boost"], durationMs: 200_000 },
  { genres: ["hiphop", "trap"], moods: ["dark_tense"], durationMs: 180_000 },
  { genres: ["rock"], moods: ["chill_relax"], durationMs: 240_000 },
  { genres: ["rock"], moods: ["energy_boost"], durationMs: 300_000 },
  { genres: [NONE], moods: [], durationMs: 100_000 },
];

test("niente selezione: nessun brano, ma i conteggi sono quelli della libreria", () => {
  const f = computeMixFacets(entries, { genres: [], moods: [], matchAll: false });
  assert.deepEqual(f.matches, []);
  assert.equal(f.totalMs, 0);
  assert.equal(f.genreCounts.get("hiphop"), 2);
  assert.equal(f.genreCounts.get(NONE), 1);
  assert.equal(f.moodCounts.energy_boost, 2);
  assert.equal(f.moodCounts.focus_study, 0);
});

test("un genere: i brani di quel genere e i mood contati solo li' dentro", () => {
  const f = computeMixFacets(entries, { genres: ["hiphop"], moods: [], matchAll: false });
  assert.deepEqual(f.matches, [0, 1]);
  assert.equal(f.totalMs, 380_000);
  assert.equal(f.moodCounts.dark_tense, 2);
  assert.equal(f.moodCounts.chill_relax, 0);
  // i generi non si restringono tra loro: scegliere Rock aggiungerebbe 2 brani
  assert.equal(f.genreCounts.get("rock"), 2);
});

test("piu' generi: basta averne uno", () => {
  const f = computeMixFacets(entries, { genres: ["trap", "rock"], moods: [], matchAll: false });
  assert.deepEqual(f.matches, [1, 2, 3]);
});

test("genere + due mood: 'Tutti insieme' puo' dare zero, 'Almeno uno' no", () => {
  const sel = { genres: ["rock"], moods: ["chill_relax", "energy_boost"] };
  const all = computeMixFacets(entries, { ...sel, matchAll: true });
  assert.deepEqual(all.matches, []);
  const any = computeMixFacets(entries, { ...sel, matchAll: false });
  assert.deepEqual(any.matches, [2, 3]);
  // i generi contano i brani che passano i mood scelti
  assert.equal(any.genreCounts.get("hiphop"), 1);
  assert.equal(all.genreCounts.get("rock") ?? 0, 0);
});

test("totali della libreria per genere e mood", () => {
  const t = computeMixTotals(entries);
  assert.equal(t.genres.get("rock"), 2);
  assert.equal(t.genres.get("trap"), 1);
  assert.equal(t.moods.dark_tense, 2);
  assert.equal(t.moods.sad_melancholy, 0);
});

test("ordine dei generi: piu' brani prima, a pari merito per nome, 'senza genere' in fondo", () => {
  const totals = new Map([
    [NONE, 50],
    ["rock", 3],
    ["trap", 1],
    ["hiphop", 3],
  ]);
  const order = sortGenreKeys(totals, (k) => k);
  assert.deepEqual(order, ["hiphop", "rock", "trap", NONE]);
});

test("riga generi chiusa: i primi N piu' quelli scelti, mai una scelta nascosta", () => {
  const ordered = ["a", "b", "c", "d", "e"];
  assert.deepEqual(visibleGenreKeys(ordered, [], 3, false), { shown: ["a", "b", "c"], hidden: 2 });
  assert.deepEqual(visibleGenreKeys(ordered, ["e"], 3, false), {
    shown: ["a", "b", "c", "e"],
    hidden: 1,
  });
  assert.deepEqual(visibleGenreKeys(ordered, [], 3, true), { shown: ordered, hidden: 0 });
  assert.deepEqual(visibleGenreKeys(["a", "b"], [], 3, false), { shown: ["a", "b"], hidden: 0 });
});

test("mood mai usati in libreria nascosti, tranne quelli scelti; 'mostra tutti' li riporta", () => {
  const totals = Object.fromEntries(TRACK_MOOD_IDS.map((id) => [id, 0]));
  totals.dark_tense = 4;
  totals.energy_boost = 1;
  const off = visibleMoodIds(totals, ["focus_study"], false);
  assert.deepEqual(off.shown, ["energy_boost", "focus_study", "dark_tense"]);
  assert.equal(off.hidden, TRACK_MOOD_IDS.length - 3);
  const all = visibleMoodIds(totals, [], true);
  assert.equal(all.shown.length, TRACK_MOOD_IDS.length);
  assert.equal(all.hidden, 0);
});

test("copertine: un brano per album, solo album con la copertina", () => {
  const tracks = [
    { id: 1, album_id: 10, art: false },
    { id: 2, album_id: 11, art: true },
    { id: 3, album_id: 12, art: true },
    { id: 4, album_id: 11, art: true },
    { id: 5, album_id: null, art: true },
    { id: 6, album_id: 13, art: true },
  ];
  assert.deepEqual(pickCoverTracks(tracks, (t) => t.art, 2).map((t) => t.id), [2, 3]);
  assert.deepEqual(pickCoverTracks(tracks, (t) => t.art, 5).map((t) => t.id), [2, 3, 6]);
  assert.deepEqual(pickCoverTracks(tracks, () => false), []);
});

test("selezione salvata: valori strani scartati, mai un'eccezione", () => {
  assert.deepEqual(parseMixSelection(null), { genres: [], moods: [], matchAll: false });
  assert.deepEqual(parseMixSelection("{rotto"), { genres: [], moods: [], matchAll: false });
  assert.deepEqual(
    parseMixSelection(
      JSON.stringify({ genres: ["rock", "rock", 3, ""], moods: ["dark_tense", "boh"], matchAll: true }),
    ),
    { genres: ["rock"], moods: ["dark_tense"], matchAll: true },
  );
  assert.deepEqual(parseMixSelection(JSON.stringify({ matchAll: "si" })), {
    genres: [],
    moods: [],
    matchAll: false,
  });
});

test("generi spariti dalla libreria tolti dalla selezione", () => {
  const sel = { genres: ["rock", "gone"], moods: ["dark_tense"], matchAll: false };
  const known = new Set(["rock"]);
  assert.deepEqual(pruneMixSelection(sel, (k) => known.has(k)).genres, ["rock"]);
  const same = { genres: ["rock"], moods: [], matchAll: false };
  assert.equal(pruneMixSelection(same, (k) => known.has(k)), same);
});
