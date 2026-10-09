/** Linux desktop shell: now-playing reaches the MPRIS player only on real changes. */
import assert from "node:assert/strict";
import { mock, test } from "node:test";

const calls = [];
globalThis.window = globalThis;
globalThis.__TAURI_INTERNALS__ = {
  invoke: async (cmd, args, options) => {
    calls.push({ cmd, args, headers: options?.headers });
    return cmd === "media_art" ? "file:///cache/media-art/c1.img" : null;
  },
  transformCallback: () => 0,
};
Object.defineProperty(globalThis, "navigator", {
  value: { userAgent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 Safari/605.1.15", platform: "Linux x86_64" },
  configurable: true,
});
let fetches = 0;
globalThis.fetch = async () => {
  fetches += 1;
  return { ok: true, arrayBuffer: async () => new Uint8Array([1, 2, 3]).buffer };
};
mock.timers.enable({ apis: ["setTimeout", "Date"], now: 1_000_000 });
const media = await import("./desktopMedia.ts");

const settle = async () => {
  mock.timers.tick(100);
  // The first flush also loads @tauri-apps/api/core (dynamic import).
  for (let i = 0; i < 40; i++) await new Promise((r) => setImmediate(r));
};
await import("@tauri-apps/api/core");
const updates = () => calls.filter((c) => c.cmd === "media_update");

test("a track change crosses the bridge once, with the cover as a local file", async () => {
  media.pushDesktopMetadata({ title: "Song", artist: "Artist", album: "Album" }, "http://hub/cover/1/256");
  media.pushDesktopPlaybackState("playing");
  media.pushDesktopPosition(200, 0);
  await settle();
  assert.equal(updates().length, 1);
  const now = updates()[0].args.now;
  assert.equal(now.title, "Song");
  assert.equal(now.playing, true);
  assert.equal(now.durationMs, 200_000);
  assert.equal(now.artUrl, "file:///cache/media-art/c1.img");
  assert.equal("artworkUrl" in now, false);
  assert.equal(fetches, 1);
});

test("periodic positions the widget can extrapolate are not sent", async () => {
  const before = updates().length;
  for (let s = 5; s <= 30; s += 5) {
    mock.timers.tick(5000);
    media.pushDesktopPosition(200, s);
    await settle();
  }
  assert.equal(updates().length, before);
});

test("pause, seek and the next track are sent; the cover is fetched once per album", async () => {
  let n = updates().length;
  media.pushDesktopPlaybackState("paused");
  await settle();
  assert.equal(updates().length, ++n);
  media.pushDesktopPosition(200, 120);
  await settle();
  assert.equal(updates().length, ++n);
  media.pushDesktopMetadata({ title: "Next", artist: "Artist", album: "Album" }, "http://hub/cover/1/256");
  media.pushDesktopPlaybackState("playing");
  media.pushDesktopPosition(180, 0);
  await settle();
  assert.equal(updates().length, ++n);
  assert.equal(fetches, 1);
});

test("an empty queue releases the player", async () => {
  media.pushDesktopPlaybackState("none");
  await settle();
  assert.equal(calls.at(-1).cmd, "media_clear");
});
