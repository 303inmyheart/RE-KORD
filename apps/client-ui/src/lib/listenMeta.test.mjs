/**
 * Studio › Listen meta line: separators only between parts that exist.
 */
import assert from "node:assert/strict";
import { test } from "node:test";
import { listenMetaParts } from "./listenMeta.ts";

const kinds = (parts) => parts.map((p) => p.kind);

test("all parts present, in order", () => {
  const parts = listenMetaParts({
    artist: "Salmo",
    album: "Death USB",
    lyrics: "lrc",
    duration: "3:21",
    plays: 4,
  });
  assert.deepEqual(kinds(parts), ["names", "lyrics", "duration", "plays"]);
  assert.equal(parts[0].text, "Salmo · Death USB");
});

test("no lyrics: no empty slot between names and duration", () => {
  const parts = listenMetaParts({
    artist: "Salmo",
    album: "Death USB",
    lyrics: "off",
    duration: "3:21",
    plays: 0,
  });
  assert.deepEqual(kinds(parts), ["names", "duration", "plays"]);
});

test("missing album or artist: single name, no dangling separator", () => {
  assert.equal(
    listenMetaParts({ artist: "Salmo", album: "  ", lyrics: "off", plays: null })[0].text,
    "Salmo",
  );
  assert.equal(
    listenMetaParts({ artist: null, album: "Death USB", lyrics: "off", plays: null })[0].text,
    "Death USB",
  );
});

test("external item without names or duration: only what exists", () => {
  assert.deepEqual(
    kinds(listenMetaParts({ artist: "", album: "", lyrics: "off", duration: null, plays: null })),
    [],
  );
  assert.deepEqual(
    kinds(listenMetaParts({ artist: "", album: "", lyrics: "plain", duration: "", plays: 2 })),
    ["lyrics", "plays"],
  );
});

test("an episode (show as artist and album) names the show once", () => {
  assert.equal(
    listenMetaParts({ artist: "Il Mondo", album: "LIVE", lyrics: "off", plays: null, external: true })[0]
      .text,
    "Il Mondo",
  );
  // A self-titled album keeps both names.
  assert.equal(
    listenMetaParts({ artist: "Salmo", album: "Salmo", lyrics: "off", plays: 0 })[0].text,
    "Salmo · Salmo",
  );
});
