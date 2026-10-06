/**
 * Crossfade state machine of the real PlayerController, on fake <audio>
 * decks and mocked timers: double `ended`, Next / pause / Space (toggle)
 * during a fade, a stuck incoming deck (watchdog) and a refused one (no
 * reload loop). Regression tests for the freeze at crossfaded track changes.
 */
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { registerHooks, stripTypeScriptTypes } from "node:module";
import { mock, test } from "node:test";
import { fileURLToPath } from "node:url";
import { compileModule } from "svelte/compiler";

// ---- Loading player.ts outside Vite -----------------------------------------
// `*.svelte.ts` stores (i18n, toasts) need the Svelte compiler, locale JSON
// needs a module wrapper and `import.meta.glob` (Vite only) has nothing to find.
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

const winEvents = new EventTarget();
Object.assign(globalThis, {
  window: globalThis,
  addEventListener: winEvents.addEventListener.bind(winEvents),
  removeEventListener: winEvents.removeEventListener.bind(winEvents),
  dispatchEvent: winEvents.dispatchEvent.bind(winEvents),
  matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }),
  localStorage: new FakeStorage(),
  sessionStorage: new FakeStorage(),
  location: new URL("http://127.0.0.1:7422/"),
  document: Object.assign(new EventTarget(), {
    visibilityState: "visible",
    hidden: false,
    documentElement: { dataset: {}, style: { setProperty() {}, removeProperty() {} } },
  }),
  HTMLMediaElement: { HAVE_NOTHING: 0, HAVE_METADATA: 1, HAVE_CURRENT_DATA: 2, HAVE_FUTURE_DATA: 3, HAVE_ENOUGH_DATA: 4 },
  MediaError: { MEDIA_ERR_ABORTED: 1, MEDIA_ERR_NETWORK: 2, MEDIA_ERR_DECODE: 3, MEDIA_ERR_SRC_NOT_SUPPORTED: 4 },
});
Object.defineProperty(globalThis, "navigator", {
  value: { userAgent: "node-test", languages: ["it"], language: "it" },
  configurable: true,
});

const TRACK_SEC = 20;
/** Paths whose `play()` never settles / rejects (set per test). */
const stalled = new Set();
const refused = new Set();

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
    this.preload = "";
    this.crossOrigin = null;
    this._src = null;
    this._volume = 1;
    this.volumeWrites = [];
    this.playCalls = 0;
    this.loads = 0;
  }
  get volume() { return this._volume; }
  set volume(v) { this._volume = v; this.volumeWrites.push(v); }
  get src() { return this._src ? new URL(this._src, location.href).href : ""; }
  set src(v) { this._src = v; }
  get currentSrc() { return this.src; }
  getAttribute(k) { return k === "src" ? this._src : null; }
  setAttribute(k, v) { if (k === "src") this._src = v; }
  removeAttribute(k) { if (k === "src") this._src = null; }
  fire(type) { this.dispatchEvent(new Event(type)); }
  load() {
    this.loads += 1;
    this.paused = true;
    this.ended = false;
    this.currentTime = 0;
    if (!this._src) {
      this.readyState = 0;
      this.duration = NaN;
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
    if ([...refused].some((r) => p.includes(r))) {
      return Promise.reject(new DOMException("refused", "NotSupportedError"));
    }
    if ([...stalled].some((r) => p.includes(r))) return new Promise(() => {});
    const was = this.paused;
    this.paused = false;
    this.ended = false;
    if (was) queueMicrotask(() => { this.fire("play"); this.fire("playing"); });
    return Promise.resolve();
  }
  pause() {
    if (this.paused) return;
    this.paused = true;
    queueMicrotask(() => this.fire("pause"));
  }
}
globalThis.Audio = FakeAudio;
globalThis.HTMLAudioElement = FakeAudio;
globalThis.Image = class { decoding = ""; src = ""; };

// Levels are planned on `performance.now()`: it follows the mocked clock.
mock.timers.enable({ apis: ["setTimeout", "setInterval", "Date"], now: 1_000_000 });
Object.defineProperty(globalThis, "performance", { value: { now: () => Date.now() }, configurable: true });
const { player } = await import("./player.ts");

// ---- Helpers -----------------------------------------------------------------
const flush = async () => {
  for (let i = 0; i < 5; i++) await new Promise((r) => setImmediate(r));
};
const tick = async (ms) => {
  for (let left = ms; left > 0; left -= 50) {
    mock.timers.tick(Math.min(50, left));
    await flush();
  }
};
const track = (n) => ({
  id: n,
  rel_path: `Artist/Album/${String(n).padStart(2, "0")} - Song ${n}.m4a`,
  title: `Song ${n}`,
  artist_name: "Artist",
  album_name: "Album",
  duration_ms: TRACK_SEC * 1000,
  track_number: n,
  album_id: 1,
  artist_id: 1,
});
const audible = () => FakeAudio.all.filter((a) => !a.paused);
/** The deck holding track `n` (the sounding one first, then the newest). */
const deckWith = (n) => {
  const decks = FakeAudio.all.filter((a) => a.path().includes(`Song ${n}.m4a`));
  return decks.find((a) => !a.paused) ?? decks.at(-1);
};
const advance = async (el, t) => {
  el.currentTime = t;
  el.fire("timeupdate");
  await flush();
};
const endDeck = async (el) => {
  el.currentTime = el.duration;
  el.ended = true;
  el.paused = true;
  el.fire("pause");
  el.fire("ended");
  await flush();
};

/** A fresh queue of five tracks, crossfade 3 s, playing track 1. */
async function start() {
  stalled.clear();
  refused.clear();
  player.applyCrossfadeSec(3);
  player.playSequence([1, 2, 3, 4, 5].map(track), 0);
  await tick(100);
  const a = deckWith(1);
  assert.ok(a && !a.paused, "track 1 plays");
  return a;
}

/** Bring track 1 into its fade window: the crossfade into track 2 starts. */
async function intoFade(a) {
  await advance(a, TRACK_SEC - 2.5);
  await flush();
  const b = deckWith(2);
  assert.ok(b, "track 2 is on the other deck");
  return b;
}

// ---- Tests -------------------------------------------------------------------
test("a natural crossfade swaps to the next track once", async () => {
  const a = await start();
  const b = await intoFade(a);
  assert.equal(audible().length, 2, "both decks sound during the fade");
  await tick(2700);
  assert.equal(player.current?.title, "Song 2");
  assert.deepEqual(audible(), [b]);
  assert.equal(player.playing, true);
});

test("a double `ended` on the outgoing deck advances only one track", async () => {
  const a = await start();
  const b = await intoFade(a);
  await endDeck(a);
  a.fire("ended");
  await flush();
  await tick(3000);
  assert.equal(player.current?.title, "Song 2");
  assert.deepEqual(audible(), [b]);
});

test("Next during a fade finishes the swap instead of skipping two", async () => {
  const a = await start();
  const b = await intoFade(a);
  await player.next();
  await flush();
  assert.equal(player.current?.title, "Song 2");
  assert.deepEqual(audible(), [b]);
  await tick(3000);
  assert.equal(player.current?.title, "Song 2");
});

test("play/pause (Space) during a fade silences both decks, then resumes the new track", async () => {
  const a = await start();
  const b = await intoFade(a);
  await player.toggle();
  await flush();
  assert.deepEqual(audible(), [], "nothing keeps playing");
  assert.equal(player.playing, false);
  assert.equal(player.current?.title, "Song 2");
  await tick(4000);
  assert.deepEqual(audible(), [], "no timer restarts a deck");
  await player.toggle();
  await flush();
  assert.deepEqual(audible(), [b]);
  assert.equal(player.playing, true);
});

test("pause from the OS media controls during a fade silences both decks", async () => {
  const a = await start();
  await intoFade(a);
  player.pause();
  await flush();
  assert.deepEqual(audible(), []);
  assert.equal(player.playing, false);
});

test("a stuck incoming deck: the watchdog settles the fade and Space still works", async () => {
  const a = await start();
  stalled.add("Song 2.m4a");
  await advance(a, TRACK_SEC - 2.5);
  await flush();
  // Within the fade nothing settles (the incoming play never resolves)…
  await tick(2000);
  // …the watchdog ends the transition: the outgoing deck goes on alone.
  await tick(7000);
  assert.deepEqual(audible(), [a]);
  await player.toggle();
  await flush();
  assert.deepEqual(audible(), [], "Space pauses");
  assert.equal(player.playing, false);
  await player.toggle();
  await flush();
  assert.deepEqual(audible(), [a], "and resumes");
});

test("Space during a stuck fade pauses instead of hanging", async () => {
  const a = await start();
  stalled.add("Song 2.m4a");
  await advance(a, TRACK_SEC - 2.5);
  await tick(500);
  await player.toggle();
  await flush();
  assert.deepEqual(audible(), []);
  assert.equal(player.playing, false);
});

test("an incoming deck that refuses to play is not reloaded on every timeupdate", async () => {
  const a = await start();
  refused.add("Song 2.m4a");
  await advance(a, TRACK_SEC - 2.5);
  const b = deckWith(2) ?? FakeAudio.all.find((x) => x !== a && x.playCalls > 0);
  const playsAfterFirst = b?.playCalls ?? 0;
  const loadsBefore = FakeAudio.all.reduce((n, x) => n + x.loads, 0);
  for (const t of [17.75, 18, 18.25, 18.5, 18.75, 19, 19.25, 19.5]) await advance(a, t);
  const loadsAfter = FakeAudio.all.reduce((n, x) => n + x.loads, 0);
  assert.equal(b?.playCalls ?? 0, playsAfterFirst, "no new crossfade attempts");
  assert.ok(loadsAfter - loadsBefore <= 2, `at most one re-warm (got ${loadsAfter - loadsBefore} loads)`);
  assert.deepEqual(audible(), [a], "the outgoing track plays on");
});

test("element volumes move in coarse steps, never rewriting the same value", async () => {
  const a = await start();
  const b = await intoFade(a);
  a.volumeWrites.length = 0;
  b.volumeWrites.length = 0;
  await tick(2700);
  for (const el of [a, b]) {
    const w = el.volumeWrites;
    for (let i = 1; i < w.length; i++) assert.notEqual(w[i], w[i - 1], "no repeated writes");
    assert.ok(w.length <= 1 / 0.02 + 2, `bounded writes per fade (got ${w.length})`);
  }
  assert.equal(b.volume, 1);
});

test("a late volume echo from the engine does not leave the new track quiet", async () => {
  const a = await start();
  const b = await intoFade(a);
  await tick(2700);
  assert.equal(player.current?.title, "Song 2");
  // WebKitGTK: the sound server reports an older step back into the element.
  b._volume = 0.15;
  b.fire("volumechange");
  await tick(400);
  assert.equal(b.volume, 1);
});
