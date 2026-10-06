/** MPRIS / Media Session: only real changes reach the OS. */
import assert from "node:assert/strict";
import { test } from "node:test";
import { positionChanged } from "./mediaPosition.ts";

const at = (position, ms, extra = {}) => ({ duration: 200, position, rate: 1, at: ms, playing: true, ...extra });

test("the first position always goes", () => {
  assert.equal(positionChanged(null, at(0, 0)), true);
});

test("a position the clock predicts is not resent while playing", () => {
  assert.equal(positionChanged(at(10, 0), at(15, 5000)), false);
  assert.equal(positionChanged(at(10, 0), at(15.9, 5000)), false);
});

test("a seek, a new track, a rate or a play/pause change is resent", () => {
  assert.equal(positionChanged(at(10, 0), at(60, 5000)), true);
  assert.equal(positionChanged(at(10, 0), at(15, 5000, { duration: 180 })), true);
  assert.equal(positionChanged(at(10, 0), at(15, 5000, { rate: 2 })), true);
  assert.equal(positionChanged(at(10, 0), at(15, 5000, { playing: false })), true);
});

test("paused: the same position is not resent", () => {
  const p = at(42, 0, { playing: false });
  assert.equal(positionChanged(p, at(42, 9000, { playing: false })), false);
  assert.equal(positionChanged(p, at(50, 9000, { playing: false })), true);
});
