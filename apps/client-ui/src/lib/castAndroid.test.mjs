/**
 * Native Android Cast bridge: status mapping and the backend over a fake
 * `window.RekordCastNative` (the Kotlin side is `RekordCast.kt`).
 *
 * Si lancia con `pnpm test` (node --experimental-strip-types).
 */
import assert from "node:assert/strict";
import { test } from "node:test";

const calls = [];
let nativeStatus = { ready: false, supported: false };
const bridge = {
  getStatus: () => JSON.stringify(nativeStatus),
  requestSession: () => calls.push(["requestSession"]),
  endSession: (stop) => calls.push(["endSession", stop]),
  load: (json) => calls.push(["load", JSON.parse(json)]),
  play: () => calls.push(["play"]),
  pause: () => calls.push(["pause"]),
  stop: () => calls.push(["stop"]),
  seek: (s) => calls.push(["seek", s]),
};
const win = new EventTarget();
win.RekordCastNative = bridge;
globalThis.window = win;

const {
  androidCastBridge,
  androidLoadPayload,
  mapAndroidCastStatus,
  mapAndroidIdleReason,
  mapAndroidPlayerState,
  parseAndroidCastEvent,
  parseAndroidCastStatus,
} = await import("./cast/androidCastStatus.ts");
const { createAndroidCastBackend } = await import("./cast/androidCast.ts");

/** What RekordCast.dispatch() does on the Kotlin side. */
const fire = (detail) => win.dispatchEvent(new CustomEvent("rekord:cast", { detail }));
const tick = () => new Promise((r) => setTimeout(r, 0));

const CONNECTED = {
  ready: true,
  supported: true,
  castState: "CONNECTED",
  deviceName: "Salotto",
  playerState: "PLAYING",
  idleReason: null,
  mediaLoaded: true,
  positionMs: 12_500,
  durationMs: 215_000,
  contentId: "http://192.168.1.20:7420/media/a.mp3",
};

test("bridge detection", () => {
  assert.equal(androidCastBridge(null), null);
  assert.equal(androidCastBridge({}), null);
  assert.equal(androidCastBridge({ RekordCastNative: { getStatus: 1 } }), null);
  assert.equal(androidCastBridge(win), bridge);
});

test("unsupported / not ready → unavailable, nothing else leaks", () => {
  const s = mapAndroidCastStatus({ ready: true, supported: false, reason: "play-services-2", castState: "CONNECTED" });
  assert.equal(s.session, "unavailable");
  assert.equal(s.deviceName, null);
  assert.equal(s.player, "idle");
  assert.equal(mapAndroidCastStatus({}).session, "unavailable");
});

test("cast state → session state", () => {
  const map = (castState) => mapAndroidCastStatus({ ready: true, supported: true, castState }).session;
  assert.equal(map("NO_DEVICES_AVAILABLE"), "unavailable");
  assert.equal(map("NOT_CONNECTED"), "available");
  assert.equal(map("CONNECTING"), "connecting");
  assert.equal(map("CONNECTED"), "connected");
  assert.equal(map("WHATEVER"), "unavailable");
});

test("connected status: device, player state, seconds, media url", () => {
  assert.deepEqual(mapAndroidCastStatus(CONNECTED), {
    session: "connected",
    deviceName: "Salotto",
    player: "playing",
    idleReason: null,
    currentTime: 12.5,
    duration: 215,
    mediaUrl: "http://192.168.1.20:7420/media/a.mp3",
  });
});

test("finished track → idle + finished (drives queue advance)", () => {
  const s = mapAndroidCastStatus({ ...CONNECTED, playerState: "IDLE", idleReason: "FINISHED", mediaLoaded: false });
  assert.equal(s.player, "idle");
  assert.equal(s.idleReason, "finished");
});

test("idle reason only while idle; player-state fallbacks", () => {
  assert.equal(mapAndroidCastStatus({ ...CONNECTED, idleReason: "FINISHED" }).idleReason, null);
  assert.equal(mapAndroidPlayerState(null, true), "paused");
  assert.equal(mapAndroidPlayerState(null, false), "idle");
  assert.equal(mapAndroidPlayerState("BUFFERING", true), "buffering");
  assert.equal(mapAndroidPlayerState("LOADING", false), "loading");
  assert.equal(mapAndroidIdleReason("CANCELLED"), "cancelled");
  assert.equal(mapAndroidIdleReason("INTERRUPTED"), "interrupted");
  assert.equal(mapAndroidIdleReason("ERROR"), "error");
  assert.equal(mapAndroidIdleReason(null), null);
});

test("bad numbers become 0", () => {
  const s = mapAndroidCastStatus({ ...CONNECTED, positionMs: -5, durationMs: Number.NaN });
  assert.equal(s.currentTime, 0);
  assert.equal(s.duration, 0);
});

test("event and status parsing", () => {
  assert.equal(parseAndroidCastStatus("nope"), null);
  assert.equal(parseAndroidCastStatus(42), null);
  assert.deepEqual(parseAndroidCastStatus('{"ready":true}'), { ready: true });
  assert.equal(parseAndroidCastEvent(null), null);
  assert.equal(parseAndroidCastEvent({ type: "session", event: "boom" }), null);
  assert.equal(parseAndroidCastEvent({ type: "result" }), null);
  assert.deepEqual(parseAndroidCastEvent({ type: "result", id: 3, ok: false, error: "x" }), {
    type: "result",
    id: 3,
    ok: false,
    error: "x",
  });
  assert.deepEqual(parseAndroidCastEvent({ type: "session", event: "startFailed", code: 2005 }), {
    type: "session",
    event: "startFailed",
    code: 2005,
  });
});

test("load payload", () => {
  const p = JSON.parse(
    androidLoadPayload(
      7,
      "http://192.168.1.20:7420/api/v1/transcode/a.flac?format=mp3",
      "audio/mpeg",
      { title: "T", artist: "A", album: "B", coverUrl: null },
      { startTime: -3 },
    ),
  );
  assert.deepEqual(p, {
    id: 7,
    url: "http://192.168.1.20:7420/api/v1/transcode/a.flac?format=mp3",
    contentType: "audio/mpeg",
    title: "T",
    artist: "A",
    album: "B",
    coverUrl: null,
    startTime: 0,
    autoplay: true,
  });
});

test("backend: waits for native init, then follows status events", async () => {
  const backend = createAndroidCastBackend();
  assert.ok(backend);
  assert.equal(backend.id, "android-native");
  const available = backend.isAvailable();
  const seen = [];
  backend.subscribe((s) => seen.push(s.session));
  fire({ type: "status", status: { ready: true, supported: true, castState: "NOT_CONNECTED" } });
  assert.equal(await available, true);
  assert.equal(backend.getStatus().session, "available");

  // Picker closed without a choice → "cancel" (castController shows no toast).
  calls.length = 0;
  const cancelled = backend.requestSession();
  await tick();
  assert.deepEqual(calls, [["requestSession"]]);
  fire({ type: "session", event: "cancelled" });
  await assert.rejects(cancelled, /cancel/);

  // Device chosen → resolves once connected.
  const session = backend.requestSession();
  await tick();
  fire({ type: "status", status: { ...CONNECTED, castState: "CONNECTING", playerState: null } });
  fire({ type: "status", status: CONNECTED });
  await session;
  assert.equal(backend.getStatus().deviceName, "Salotto");
  assert.deepEqual(seen.slice(-2), ["connecting", "connected"]);

  // Load result is matched by id.
  calls.length = 0;
  const load = backend.loadMedia(
    "http://192.168.1.20:7420/media/a.mp3",
    "audio/mpeg",
    { title: "T", artist: "A", album: "B", coverUrl: "http://192.168.1.20:7420/api/v1/covers/album/1" },
    { startTime: 42, autoplay: false },
  );
  const [[name, payload]] = calls;
  assert.equal(name, "load");
  assert.equal(payload.startTime, 42);
  assert.equal(payload.autoplay, false);
  fire({ type: "result", id: payload.id + 1000, ok: false });
  fire({ type: "result", id: payload.id, ok: true });
  await load;

  const failed = backend.loadMedia("u", "audio/mpeg", { title: "", artist: "", album: "", coverUrl: null }, { startTime: 0 });
  const failedId = calls.at(-1)[1].id;
  fire({ type: "result", id: failedId, ok: false, error: "LOAD_FAILED" });
  await assert.rejects(failed, /LOAD_FAILED/);

  calls.length = 0;
  await backend.play();
  await backend.pause();
  await backend.seek(-4);
  await backend.seek(Number.NaN);
  await backend.stop();
  await backend.endSession();
  assert.deepEqual(calls, [["play"], ["pause"], ["seek", 0], ["stop"], ["endSession", true]]);
});

test("backend: start failure carries the native code", async () => {
  nativeStatus = { ready: true, supported: true, castState: "NOT_CONNECTED" };
  const backend = createAndroidCastBackend({
    sessionFailed: (code) => `failed ${code}`,
    timeout: () => "timeout",
  });
  assert.equal(await backend.isAvailable(), true);
  const p = backend.requestSession();
  await tick();
  fire({ type: "session", event: "startFailed", code: 2005 });
  await assert.rejects(p, /failed 2005/);
});

test("backend: no Play Services → not available", async () => {
  nativeStatus = { ready: true, supported: false, reason: "play-services-1" };
  const backend = createAndroidCastBackend();
  assert.equal(await backend.isAvailable(), false);
  await assert.rejects(backend.requestSession(), /cast-unavailable/);
});
