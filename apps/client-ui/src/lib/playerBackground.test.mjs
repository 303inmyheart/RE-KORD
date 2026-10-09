/**
 * Background playback (Android, screen off) on the real PlayerController, with
 * fake <audio> decks, mocked timers and a fake `window.RekordMediaNative`:
 * a stalled stream is reconnected at its position, a `play()` refused in the
 * background is retried, a network error with the hub reachable reconnects
 * instead of skipping, a lost `ended` still advances, and the shell hears what
 * the player means to do (`wantsPlay`) and why it paused (`pauseReason`).
 */
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { registerHooks, stripTypeScriptTypes } from "node:module";
import { mock, test } from "node:test";
import { fileURLToPath } from "node:url";
import { compileModule } from "svelte/compiler";

// ---- Loading player.ts outside Vite (same hooks as playerCrossfade.test.mjs) --
registerHooks({
  resolve(specifier, context, next) {
    if (specifier.startsWith(".") && specifier.endsWith(".svelte")) {
      const guess = `${specifier}.ts`;
      if (existsSync(fileURLToPath(new URL(guess, context.parentURL)))) return next(guess, context);
    }
    return next(specifier, context);
  },
  load(url, context, next) {
    if (url.startsWith("file:") && url.endsWith(".svelte.ts")) {
      const file = fileURLToPath(url);
      const src = stripTypeScriptTypes(readFileSync(file, "utf8")).replace(
        /import\.meta\.glob\s*\([^)]*\)/g,
        "({})",
      );
      const out = compileModule(src, { generate: "client", filename: file });
      return { format: "module", source: out.js.code, shortCircuit: true };
    }
    if (url.startsWith("file:") && url.endsWith(".json")) {
      return {
        format: "module",
        source: `export default ${readFileSync(fileURLToPath(url), "utf8")};`,
        shortCircuit: true,
      };
    }
    return next(url, context);
  },
});

// ---- Browser stand-ins -------------------------------------------------------
class FakeStorage {
  #m = new Map();
  getItem(k) { return this.#m.has(k) ? this.#m.get(k) : null; }
  setItem(k, v) { this.#m.set(k, String(v)); }
  removeItem(k) { this.#m.delete(k); }
  clear() { this.#m.clear(); }
  key(i) { return [...this.#m.keys()][i] ?? null; }
  get length() { return this.#m.size; }
}

/** What crossed the Android bridge. */
const updates = [];
const logs = [];
const winEvents = new EventTarget();
const doc = Object.assign(new EventTarget(), {
  visibilityState: "visible",
  hidden: false,
  documentElement: { dataset: {}, style: { setProperty() {}, removeProperty() {} } },
});
Object.assign(globalThis, {
  window: globalThis,
  addEventListener: winEvents.addEventListener.bind(winEvents),
  removeEventListener: winEvents.removeEventListener.bind(winEvents),
  dispatchEvent: winEvents.dispatchEvent.bind(winEvents),
  matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }),
  localStorage: new FakeStorage(),
  sessionStorage: new FakeStorage(),
  location: new URL("http://127.0.0.1:7422/"),
  document: doc,
  HTMLMediaElement: { HAVE_NOTHING: 0, HAVE_METADATA: 1, HAVE_CURRENT_DATA: 2, HAVE_FUTURE_DATA: 3, HAVE_ENOUGH_DATA: 4 },
  MediaError: { MEDIA_ERR_ABORTED: 1, MEDIA_ERR_NETWORK: 2, MEDIA_ERR_DECODE: 3, MEDIA_ERR_SRC_NOT_SUPPORTED: 4 },
  RekordMediaNative: {
    update: (json) => updates.push(JSON.parse(json)),
    stop: () => updates.push(null),
    log: (m) => logs.push(m),
  },
});
Object.defineProperty(globalThis, "navigator", {
  value: { userAgent: "node-test", languages: ["it"], language: "it", onLine: true },
  configurable: true,
});
/** The hub answers /health (a network error then means a broken connection). */
let hubUp = true;
globalThis.fetch = async () => {
  if (!hubUp) throw new TypeError("Failed to fetch");
  return new Response(JSON.stringify({ service: "RE-KORD", version: "5.1.0" }), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
};

const TRACK_SEC = 40;
/** Paths whose `play()` is refused once (consumed) / never settles. */
const refuseOnce = new Set();
const stalled = new Set();
/** Paths whose load never gets an answer (dead link): no metadata, no data. */
const hung = new Set();

class FakeAudio extends EventTarget {
  static all = [];
  constructor() {
    super();
    FakeAudio.all.push(this);
    this.paused = true;
    this.currentTime = 0;
    this.duration = NaN;
    this.ended = false;
    this.error = null;
    this.readyState = 0;
    this.networkState = 0;
    this.seeking = false;
    this.preload = "";
    this.crossOrigin = null;
    this.loop = false;
    this._src = null;
    this._volume = 1;
    this.playCalls = 0;
    this.loads = 0;
    this.pendingPlays = [];
  }
  /** Like browsers: pause() and load() reject a play() still pending. */
  abortPendingPlays() {
    const pending = this.pendingPlays;
    this.pendingPlays = [];
    for (const reject of pending) reject(new DOMException("interrupted", "AbortError"));
  }
  get volume() { return this._volume; }
  set volume(v) { this._volume = v; }
  get src() { return this._src ? new URL(this._src, location.href).href : ""; }
  set src(v) { this._src = v; }
  get currentSrc() { return this.src; }
  getAttribute(k) { return k === "src" ? this._src : null; }
  setAttribute(k, v) { if (k === "src") this._src = v; }
  removeAttribute(k) { if (k === "src") this._src = null; }
  fire(type) { this.dispatchEvent(new Event(type)); }
  load() {
    this.loads += 1;
    this.abortPendingPlays();
    this.paused = true;
    this.ended = false;
    this.error = null;
    this.currentTime = 0;
    if (!this._src || [...hung].some((r) => this.path().includes(r))) {
      this.readyState = 0;
      this.duration = NaN;
      // The position went back to 0: browsers say so with a timeupdate.
      if (this._src) queueMicrotask(() => this.fire("timeupdate"));
      return;
    }
    this.readyState = 4;
    this.duration = TRACK_SEC;
    queueMicrotask(() => {
      this.fire("loadedmetadata");
      this.fire("durationchange");
      this.fire("canplay");
    });
  }
  path() { return decodeURIComponent(this._src ?? ""); }
  play() {
    this.playCalls += 1;
    const p = this.path();
    const refused = [...refuseOnce].find((r) => p.includes(r));
    if (refused) {
      refuseOnce.delete(refused);
      return Promise.reject(new DOMException("not now", "NotAllowedError"));
    }
    if ([...stalled].some((r) => p.includes(r))) {
      this.paused = false;
      return new Promise((_, reject) => this.pendingPlays.push(reject));
    }
    const was = this.paused;
    this.paused = false;
    this.ended = false;
    if (was) queueMicrotask(() => { this.fire("play"); this.fire("playing"); });
    return Promise.resolve();
  }
  pause() {
    this.abortPendingPlays();
    if (this.paused) return;
    this.paused = true;
    queueMicrotask(() => this.fire("pause"));
  }
}
globalThis.Audio = FakeAudio;
globalThis.HTMLAudioElement = FakeAudio;
globalThis.Image = class { decoding = ""; src = ""; };

mock.timers.enable({ apis: ["setTimeout", "setInterval", "Date"], now: 1_000_000 });
Object.defineProperty(globalThis, "performance", { value: { now: () => Date.now() }, configurable: true });
const { player } = await import("./player.ts");

// ---- Helpers -----------------------------------------------------------------
const flush = async () => {
  for (let i = 0; i < 8; i++) await new Promise((r) => setImmediate(r));
};
const tick = async (ms) => {
  for (let left = ms; left > 0; left -= 100) {
    mock.timers.tick(Math.min(100, left));
    await flush();
  }
};
const track = (n) => ({
  id: n,
  rel_path: `Artist/Album/${String(n).padStart(2, "0")} - Song ${n}.mp3`,
  title: `Song ${n}`,
  artist_name: "Artist",
  album_name: "Album",
  duration_ms: TRACK_SEC * 1000,
  track_number: n,
  album_id: 1,
  artist_id: 1,
});
const audible = () => FakeAudio.all.filter((a) => !a.paused);
const deckWith = (n) => {
  const decks = FakeAudio.all.filter((a) => a.path().includes(`Song ${n}.mp3`));
  return decks.find((a) => !a.paused) ?? decks.at(-1);
};
const advance = async (el, t) => {
  el.currentTime = t;
  el.fire("timeupdate");
  await flush();
};
const heartbeat = async () => {
  dispatchEvent(new CustomEvent("rekord:playback-watchdog", { detail: { stalledMs: 0 } }));
  await flush();
};
const lastUpdate = () => updates.filter(Boolean).at(-1);
const hide = (hidden) => {
  doc.visibilityState = hidden ? "hidden" : "visible";
  doc.hidden = hidden;
};

/** A fresh queue of four tracks, no crossfade, playing track 1, screen off. */
async function start({ crossfade = 0 } = {}) {
  refuseOnce.clear();
  stalled.clear();
  hung.clear();
  hubUp = true;
  hide(false);
  player.applyCrossfadeSec(crossfade);
  player.playSequence([1, 2, 3, 4].map(track), 0);
  await tick(200);
  const a = deckWith(1);
  assert.ok(a && !a.paused, "track 1 plays");
  hide(true);
  logs.length = 0;
  return a;
}

// ---- Tests -------------------------------------------------------------------
test("the shell hears 'wants to play' from the tap, before the first sound", async () => {
  hide(false);
  updates.length = 0;
  stalled.add("Song 7.mp3");
  player.playSequence([track(7), track(8)], 0);
  await tick(200);
  const u = lastUpdate();
  assert.equal(u.title, "Song 7");
  assert.equal(u.playing, false, "nothing sounds yet");
  assert.equal(u.wantsPlay, true, "but the player means to play");
  stalled.clear();
  player.clearQueue();
  await tick(200);
});

test("tracks advance across boundaries with the screen off, next deck warmed early", async () => {
  const a = await start();
  // The next track is opened 20 s before the end, not at the boundary.
  await advance(a, TRACK_SEC - 21);
  assert.equal(deckWith(2), undefined, "not yet");
  await advance(a, TRACK_SEC - 19);
  const b = deckWith(2);
  assert.ok(b && b.paused && b.readyState === 4, "track 2 buffered on the other deck");
  const loads = b.loads;
  a.currentTime = TRACK_SEC;
  a.ended = true;
  a.paused = true;
  a.fire("pause");
  a.fire("ended");
  await tick(200);
  assert.equal(player.current?.title, "Song 2");
  assert.deepEqual(audible(), [b]);
  assert.equal(b.loads, loads, "no second request at the boundary");
  assert.equal(lastUpdate().playing, true);
  assert.equal(lastUpdate().wantsPlay, true);
});

test("a stream that stops progressing is reconnected at its position", async () => {
  const a = await start();
  await advance(a, 15);
  const loads = a.loads;
  // Wi-Fi dozed: the element keeps 'playing' but time no longer moves.
  a.fire("waiting");
  await tick(5000);
  await heartbeat();
  assert.equal(a.loads, loads, "not yet: a short wait is normal buffering");
  await tick(8000);
  await heartbeat();
  await tick(300);
  assert.ok(a.loads > loads, "reloaded");
  assert.equal(player.current?.title, "Song 1", "same track, not skipped");
  assert.equal(a.currentTime, 15, "at the position where it stopped");
  assert.ok(!a.paused, "and playing");
  assert.ok(logs.some((m) => /no progress for \d+s, reconnecting at 15s/.test(m)), logs.join("\n"));
});

test("a play() refused in the background at a track change is retried", async () => {
  const a = await start();
  refuseOnce.add("Song 2.mp3");
  await advance(a, TRACK_SEC - 10);
  a.currentTime = TRACK_SEC;
  a.ended = true;
  a.paused = true;
  a.fire("ended");
  await tick(200);
  assert.equal(player.current?.title, "Song 2");
  assert.equal(player.playing, false);
  assert.equal(lastUpdate().wantsPlay, true, "still means to play: the shell keeps it awake");
  await tick(3500);
  await heartbeat();
  await tick(200);
  assert.equal(player.playing, true, `playing after the retry\n${logs.join("\n")}`);
  assert.ok(!deckWith(2).paused);
  assert.ok(logs.some((m) => m.includes("refused in the background")), logs.join("\n"));
});

test("a network error with the hub reachable reconnects instead of skipping", async () => {
  const a = await start();
  await advance(a, 22);
  const loads = a.loads;
  a.error = { code: 2 };
  a.fire("error");
  await tick(500);
  assert.equal(player.current?.title, "Song 1", "not skipped");
  assert.ok(!a.paused && a.loads > loads, `reloaded and playing\n${logs.join("\n")}`);
  assert.equal(a.currentTime, 22);
  assert.ok(logs.some((m) => m.includes("network error at 22s, hub reachable")), logs.join("\n"));
});

test("an `ended` that never reached the player still advances", async () => {
  const a = await start();
  await advance(a, TRACK_SEC - 1);
  a.currentTime = TRACK_SEC;
  a.ended = true;
  a.paused = true; // …and no event at all.
  await tick(10_000);
  await heartbeat();
  await tick(200);
  assert.equal(player.current?.title, "Song 2");
  assert.equal(player.playing, true);
});

test("a pause the player did not ask for is reported as external, a user pause as user", async () => {
  const a = await start();
  // Chromium on an audio focus loss: the element pauses by itself.
  a.pause();
  await tick(1200);
  assert.equal(lastUpdate().playing, false);
  assert.equal(lastUpdate().pauseReason, "external");
  assert.equal(lastUpdate().wantsPlay, false, "the shell decides whether to resume");
  // The pause is not a stall: the watch leaves it alone.
  await tick(6000);
  await heartbeat();
  assert.ok(a.paused, "no automatic play() from the page");
  // Resumed (by the shell's play command), then paused by the user.
  dispatchEvent(new CustomEvent("rekord:media-action", { detail: { action: "play" } }));
  await tick(1200);
  assert.equal(lastUpdate().playing, true);
  assert.equal(lastUpdate().pauseReason, "");
  player.pause();
  await tick(1200);
  assert.equal(lastUpdate().pauseReason, "user");
});

test("pause from the car while already interrupted becomes a user pause", async () => {
  const a = await start();
  a.pause();
  await tick(1200);
  assert.equal(lastUpdate().pauseReason, "external");
  dispatchEvent(new CustomEvent("rekord:media-action", { detail: { action: "pause" } }));
  await tick(1200);
  assert.equal(lastUpdate().pauseReason, "user");
});

test("a network change reconnects a stalled stream at once", async () => {
  const a = await start();
  await advance(a, 30);
  const loads = a.loads;
  await tick(3000);
  dispatchEvent(new CustomEvent("rekord:network", { detail: { available: true, transport: "cellular" } }));
  await tick(300);
  assert.ok(a.loads > loads, "reloaded on the new network");
  assert.equal(a.currentTime, 30);
  assert.ok(!a.paused);
});

test("a crossfade completes on media events alone (timers frozen with the screen off)", async () => {
  const a = await start({ crossfade: 3 });
  await advance(a, TRACK_SEC - 2.8);
  const b = deckWith(2);
  assert.ok(b && !b.paused, "the incoming deck started");
  // No timer runs: only the outgoing deck's own events arrive.
  await advance(a, TRACK_SEC - 1);
  a.currentTime = TRACK_SEC;
  a.ended = true;
  a.paused = true;
  a.fire("pause");
  a.fire("ended");
  await flush();
  assert.equal(player.current?.title, "Song 2");
  assert.deepEqual(audible(), [b]);
  assert.equal(player.playing, true);
  player.applyCrossfadeSec(0);
});

test("a second reconnect during a dead link keeps the position (not back to 0)", async () => {
  const a = await start();
  await advance(a, 15);
  hung.add("Song 1.mp3");
  stalled.add("Song 1.mp3");
  await tick(13_000);
  await heartbeat();
  await tick(20_000);
  for (let i = 0; i < 4; i++) {
    await heartbeat();
    await tick(10_000);
  }
  const reconnects = logs.filter((m) => m.includes("reconnecting at"));
  assert.ok(reconnects.length >= 2, logs.join("\n"));
  for (const m of reconnects) assert.match(m, /reconnecting at 15s/);
  assert.equal(player.currentTime, 15, "the timeline keeps the position meanwhile");
  // The link comes back: the next reconnect resumes at 15 s.
  hung.clear();
  stalled.clear();
  await tick(30_000);
  await heartbeat();
  await tick(500);
  assert.equal(a.currentTime, 15);
  assert.ok(!a.paused);
});

test("hub gone: waits with the intent to play, no reload storm, resumes where it was", async () => {
  const a = await start();
  await advance(a, 22);
  const loads = a.loads;
  hubUp = false;
  a.error = { code: 2 };
  a.fire("error");
  await tick(500);
  assert.equal(player.waitingForHub, true);
  assert.equal(player.playing, false);
  assert.equal(lastUpdate().wantsPlay, true, "the shell keeps the service up for the return");
  for (let i = 0; i < 6; i++) {
    await tick(10_000);
    await heartbeat();
  }
  assert.equal(a.loads, loads, "the watch leaves an outage to the session");
  hubUp = true;
  player.resumeAfterOutage();
  await tick(500);
  assert.equal(player.playing, true);
  assert.equal(a.currentTime, 22);
});

test("a reconnect that fails because the hub is gone keeps the intent to play", async () => {
  const a = await start();
  await advance(a, 15);
  hung.add("Song 1.mp3");
  stalled.add("Song 1.mp3");
  await tick(13_000);
  await heartbeat();
  assert.ok(logs.some((m) => m.includes("reconnecting at 15s")), logs.join("\n"));
  // The reconnect's request is refused: the element reports an error.
  hubUp = false;
  a.error = { code: 4 };
  a.fire("error");
  await tick(500);
  assert.equal(player.waitingForHub, true);
  assert.equal(lastUpdate().wantsPlay, true);
  hung.clear();
  stalled.clear();
  hubUp = true;
  player.resumeAfterOutage();
  await tick(500);
  assert.equal(player.playing, true, "playing again, not left paused");
  assert.equal(a.currentTime, 15);
});

test("'format not supported' with the hub reachable still falls back to the transcoder", async () => {
  const a = await start();
  await advance(a, 5);
  a.error = { code: 4 };
  a.fire("error");
  await tick(500);
  const playing = audible();
  assert.equal(playing.length, 1);
  assert.match(playing[0].path(), /transcode/);
  assert.equal(player.current?.title, "Song 1");
});

test("while the shell reports another app's audio, the watch does not fight for it", async () => {
  const a = await start();
  refuseOnce.add("Song 2.mp3");
  await advance(a, TRACK_SEC - 10);
  a.currentTime = TRACK_SEC;
  a.ended = true;
  a.paused = true;
  a.fire("ended");
  await tick(200);
  assert.equal(player.playing, false, "refused: waiting to retry");
  const plays = deckWith(2).playCalls;
  for (let i = 0; i < 3; i++) {
    dispatchEvent(new CustomEvent("rekord:playback-watchdog", { detail: { stalledMs: 0, otherAudio: true } }));
    await tick(10_000);
  }
  assert.equal(deckWith(2).playCalls, plays, "no play() while the other audio lasts");
  await heartbeat();
  await tick(3500);
  await heartbeat();
  await tick(200);
  assert.equal(player.playing, true, "retried once it is over");
});

test("pause while waiting for the hub: the shell is told, no idle watch timer", async () => {
  const a = await start();
  await advance(a, 12);
  hubUp = false;
  a.error = { code: 2 };
  a.fire("error");
  await tick(500);
  assert.equal(player.waitingForHub, true);
  assert.equal(lastUpdate().wantsPlay, true);
  await player.toggle();
  await tick(200);
  assert.equal(lastUpdate().wantsPlay, false, "a pause during the outage reaches the shell");
  await player.toggle();
  await tick(200);
  assert.equal(lastUpdate().wantsPlay, true, "and so does play");
  hubUp = true;
  player.resumeAfterOutage();
  await tick(500);
  assert.equal(player.playing, true);
});
