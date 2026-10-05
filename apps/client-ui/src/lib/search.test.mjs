/** Ricerca lato client: titolo senza numero davanti, artista, album, generi; mai il percorso. */
import assert from "node:assert/strict";
import { test } from "node:test";
import { buildSearchIndex, searchTracks, stripTrackNumberPrefix, trackMatchesQuery } from "./search.ts";

const tr = (title, artist, album, genre = null, extra = {}) => ({
  title, artist_name: artist, album_name: album, genre, album_id: 1, rel_path: `${artist}/${album}/${title}.flac`, ...extra,
});

test("il numero di traccia davanti al titolo non conta, un titolo numerico si'", () => {
  assert.equal(stripTrackNumberPrefix("01 - In the Evening"), "In the Evening");
  assert.equal(stripTrackNumberPrefix("05. Song"), "Song");
  assert.equal(stripTrackNumberPrefix("1-03 Song"), "Song");
  assert.equal(stripTrackNumberPrefix("21 Guns"), "21 Guns");
  assert.equal(stripTrackNumberPrefix("1999"), "1999");
});

test("'01' non trova tutti i brani numerati, e il percorso non conta", () => {
  const tracks = [tr("01 - In the Evening", "Led Zeppelin", "In Through"), tr("02 - South Bound", "Led Zeppelin", "In Through")];
  const index = buildSearchIndex(tracks);
  assert.deepEqual(searchTracks(index, "01"), []);
  assert.equal(searchTracks(index, "evening").length, 1);
  assert.equal(trackMatchesQuery(tracks[0], "flac"), false);
});

test("si cerca per genere, con qualunque grafia, e per piu' parole insieme", () => {
  const tracks = [tr("Song A", "Salmo", "Ranch", "Hip-Hop; Rap"), tr("Song B", "Eagles", "Hotel", "Rock")];
  const index = buildSearchIndex(tracks);
  assert.deepEqual(searchTracks(index, "hip hop").map((t) => t.title), ["Song A"]);
  assert.deepEqual(searchTracks(index, "salmo rap").map((t) => t.title), ["Song A"]);
  assert.equal(index.artistGenreMatches("Salmo", "hiphop"), true);
  assert.equal(index.artistGenreMatches("Eagles", "hiphop"), false);
  assert.equal(index.albumGenreMatches({ id: 1, name: "x", artist_name: "y", genre: "Rock" }, "rock"), true);
});

test("prima chi ha la parola nel titolo", () => {
  const tracks = [tr("Other", "Love Band", "X"), tr("Love", "Someone", "Y")];
  assert.equal(searchTracks(buildSearchIndex(tracks), "love")[0].title, "Love");
});
