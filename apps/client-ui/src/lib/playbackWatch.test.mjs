/**
 * PlaybackWatch on its own: a clock and samples, no player. The decisions behind
 * background playback recovery (stall, refused play, lost `ended`, give up).
 */
import assert from "node:assert/strict";
import { test } from "node:test";

const { PlaybackWatch } = await import("./playbackWatch.ts");

const sample = (over = {}) => ({
  wantsPlay: true,
  busy: false,
  paused: false,
  ended: false,
  currentTime: 10,
  error: false,
  ...over,
});

test("progress keeps it quiet", () => {
  const w = new PlaybackWatch();
  w.reset(0);
  for (let t = 0; t < 60_000; t += 5_000) {
    assert.equal(w.check(t, sample({ currentTime: 10 + t / 1000 })).kind, "none");
  }
});

test("no progress while not paused: reload after the stall time, then backoff", () => {
  const w = new PlaybackWatch();
  w.reset(0);
  assert.equal(w.check(0, sample()).kind, "none");
  assert.equal(w.check(11_000, sample()).kind, "none");
  const first = w.check(12_000, sample());
  assert.equal(first.kind, "reload");
  assert.equal(first.attempt, 1);
  // Within the backoff nothing new, even if still stuck.
  assert.equal(w.check(13_000, sample()).kind, "none");
  const second = w.check(26_000, sample());
  assert.equal(second.kind, "reload");
  assert.equal(second.attempt, 2);
});

test("a late check (throttled timers) decides from the clock at once", () => {
  const w = new PlaybackWatch();
  w.reset(0);
  w.check(0, sample());
  assert.equal(w.check(90_000, sample()).kind, "reload");
});

test("after a network change the stall limit is short", () => {
  const w = new PlaybackWatch();
  w.reset(0);
  w.check(0, sample());
  assert.equal(w.check(1_500, sample(), true).kind, "none");
  assert.equal(w.check(2_500, sample(), true).kind, "reload");
});

test("paused while it should play: play() twice, then reload", () => {
  const w = new PlaybackWatch();
  w.reset(0);
  const s = sample({ paused: true });
  w.check(0, s);
  assert.equal(w.check(2_000, s).kind, "none");
  assert.deepEqual(w.check(3_000, s).kind, "play");
  assert.equal(w.check(4_000, s).kind, "none", "backoff");
  assert.equal(w.check(5_000, s).kind, "play");
  assert.equal(w.check(9_000, s).kind, "reload");
});

test("an element error is reloaded quickly", () => {
  const w = new PlaybackWatch();
  w.reset(0);
  w.check(0, sample({ error: true }));
  assert.equal(w.check(3_000, sample({ error: true })).kind, "reload");
});

test("an `ended` that led nowhere moves on", () => {
  const w = new PlaybackWatch();
  w.reset(0);
  const s = sample({ ended: true, paused: true, currentTime: 200 });
  w.check(0, s);
  assert.equal(w.check(3_000, s).kind, "none");
  assert.equal(w.check(4_000, s).kind, "advance");
});

test("busy (a load the player handles) is not a stall, until it is stuck too", () => {
  const w = new PlaybackWatch();
  w.reset(0);
  for (let t = 0; t <= 20_000; t += 5_000) {
    assert.equal(w.check(t, sample({ busy: true })).kind, "none");
  }
  // Past the busy cap the watch measures again from the last busy check.
  assert.equal(w.check(30_000, sample({ busy: true })).kind, "none");
  assert.equal(w.check(42_000, sample({ busy: true })).kind, "reload");
});

test("not meaning to play resets everything", () => {
  const w = new PlaybackWatch();
  w.reset(0);
  w.check(0, sample());
  w.check(12_000, sample());
  assert.equal(w.attemptCount, 1);
  assert.equal(w.check(13_000, sample({ wantsPlay: false })).kind, "none");
  assert.equal(w.attemptCount, 0);
  w.check(14_000, sample());
  assert.equal(w.check(20_000, sample()).kind, "none");
});

test("healthy again for a while: the next stall starts from attempt 1", () => {
  const w = new PlaybackWatch();
  w.reset(0);
  w.check(0, sample());
  assert.equal(w.check(12_000, sample()).attempt, 1);
  for (let t = 14_000; t <= 40_000; t += 2_000) w.check(t, sample({ currentTime: t / 1000 }));
  const again = w.check(60_000, sample({ currentTime: 40 }));
  assert.equal(again.kind, "reload");
  assert.equal(again.attempt, 1);
});

test("nothing plays for ten minutes despite retries: give up", () => {
  const w = new PlaybackWatch();
  w.reset(0);
  w.check(0, sample());
  let last;
  for (let t = 5_000; t <= 11 * 60_000; t += 5_000) {
    last = w.check(t, sample());
    if (last.kind === "giveUp") break;
  }
  assert.equal(last.kind, "giveUp");
});
