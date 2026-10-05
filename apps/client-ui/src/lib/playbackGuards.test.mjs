/** Regole temporanee sul player (Plectr): attesa a fine brano, dissolvenza forzata, seek bloccato. */
import assert from "node:assert/strict";
import { test } from "node:test";
import { PlaybackGuards } from "./playbackGuards.ts";

test("senza regole: si avanza, si fa seek, vale la dissolvenza dell'utente", () => {
  const g = new PlaybackGuards();
  assert.equal(g.advanceOnEnd(), true);
  assert.equal(g.allowSeek(), true);
  assert.equal(g.crossfade(3), 3);
});

test("in attesa non si avanza a fine brano; il motivo vuoto libera", () => {
  const g = new PlaybackGuards();
  assert.equal(g.setHold("plectr"), true);
  assert.equal(g.advanceOnEnd(), false);
  assert.equal(g.setHold("plectr"), false);
  g.setHold("  ");
  assert.equal(g.held, false);
});

test("dissolvenza forzata a 0 senza toccare l'impostazione; null la toglie", () => {
  const g = new PlaybackGuards();
  g.setCrossfadeOverride(0);
  assert.equal(g.crossfade(5), 0);
  g.setCrossfadeOverride(NaN);
  assert.equal(g.crossfade(5), 5);
  g.setCrossfadeOverride(99);
  assert.equal(g.crossfade(5), 30);
});

test("seek bloccato per l'utente, ammesso con force", () => {
  const g = new PlaybackGuards();
  g.setSeekLock("plectr");
  assert.equal(g.allowSeek(), false);
  assert.equal(g.allowSeek({ force: true }), true);
  g.setSeekLock(null);
  assert.equal(g.allowSeek(), true);
});
