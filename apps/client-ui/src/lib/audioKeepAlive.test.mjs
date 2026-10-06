/** Keep-alive of the shared audio output (Linux shell with the WebKit mixer). */
import assert from "node:assert/strict";
import { mock, test } from "node:test";
import { OutputKeepAlive, sharedOutputKeepAlive } from "./audioKeepAlive.ts";

function fakeContext() {
  const calls = [];
  return {
    calls,
    state: "running",
    async resume() { calls.push("resume"); this.state = "running"; },
    async suspend() { calls.push("suspend"); this.state = "suspended"; },
  };
}

test("created once, on the first wake; never before playback", () => {
  let made = 0;
  const ctx = fakeContext();
  const k = new OutputKeepAlive({ create: () => { made++; return ctx; }, idleMs: 1000 });
  assert.equal(k.state, "none");
  k.sync(false);
  assert.equal(made, 0);
  k.wake();
  k.wake();
  k.sync(true);
  assert.equal(made, 1);
});

test("pause / resume within the idle window never suspends (no cork)", () => {
  mock.timers.enable({ apis: ["setTimeout"] });
  try {
    const ctx = fakeContext();
    const k = new OutputKeepAlive({ create: () => ctx, idleMs: 15_000 });
    k.sync(true);
    for (let i = 0; i < 10; i++) {
      k.sync(false);
      mock.timers.tick(1_500);
      k.sync(true);
      mock.timers.tick(1_500);
    }
    assert.deepEqual(ctx.calls, []);
    assert.equal(ctx.state, "running");
  } finally {
    mock.timers.reset();
  }
});

test("a long pause suspends it; playing again resumes it", async () => {
  mock.timers.enable({ apis: ["setTimeout"] });
  try {
    const ctx = fakeContext();
    const k = new OutputKeepAlive({ create: () => ctx, idleMs: 15_000 });
    k.sync(true);
    k.sync(false);
    mock.timers.tick(14_999);
    assert.equal(ctx.state, "running");
    mock.timers.tick(1);
    await Promise.resolve();
    assert.equal(ctx.state, "suspended");
    k.sync(true);
    await Promise.resolve();
    assert.equal(ctx.state, "running");
    assert.deepEqual(ctx.calls, ["suspend", "resume"]);
  } finally {
    mock.timers.reset();
  }
});

test("no Web Audio: it gives up once and never throws", () => {
  let tries = 0;
  const k = new OutputKeepAlive({ create: () => { tries++; throw new Error("no"); }, idleMs: 10 });
  k.wake();
  k.sync(true);
  k.sync(false);
  k.wake();
  assert.equal(tries, 1);
  assert.equal(k.state, "none");
});

test("only with the shell's shared-output flag", () => {
  const saved = globalThis.window;
  try {
    globalThis.window = { AudioContext: class { state = "suspended"; } };
    assert.equal(sharedOutputKeepAlive(), null);
    globalThis.window.__REKORD_SHARED_AUDIO_OUTPUT__ = true;
    assert.ok(sharedOutputKeepAlive() instanceof OutputKeepAlive);
  } finally {
    globalThis.window = saved;
  }
});
