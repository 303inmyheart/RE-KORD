/**
 * Canvas loop pacing, adaptive quality, Nebula cadence and the DiscoWall
 * pixel path (no DOM).
 *
 * Run with `pnpm test` (node --experimental-strip-types).
 */
import assert from "node:assert/strict";
import { test } from "node:test";

const { AdaptiveQuality, frameSlackMs, isFrameDue, nextFrameDelay, scaledDpr } = await import(
  "./visualizer/adaptiveQuality.ts"
);
const {
  nebulaLoopCadence,
  NEBULA_ACTIVE_FRAME_MS,
  NEBULA_CALM_FRAME_MS,
  NEBULA_FOCUS_FRAME_MS,
  NEBULA_IDLE_FRAME_MS,
  NEBULA_PREVIEW_FRAME_MS,
} = await import("./visualizer/renderQuality.ts");
const { DiscoWallRenderer, hslToRgbInto, packRgb } = await import("./visualizer/discowall.ts");

test("frame pacing: a throttled loop draws once the interval (minus slack) has passed", () => {
  assert.equal(frameSlackMs(100), 10);
  assert.equal(frameSlackMs(33), 33 * 0.15);
  assert.equal(isFrameDue(1000, 1000, 100), false);
  assert.equal(isFrameDue(1080, 1000, 100), false);
  assert.equal(isFrameDue(1088, 1000, 100), true);
  assert.equal(isFrameDue(0, -Infinity, 100), true);
  // Wakes early by the slack, never sooner than 1 ms.
  assert.equal(nextFrameDelay(1000, 1000, 100), 90);
  assert.equal(nextFrameDelay(1200, 1000, 100), 1);
  assert.equal(nextFrameDelay(0, -Infinity, 33), 1);
});

test("scaledDpr: engine cap, adaptive step, never below 1x", () => {
  assert.equal(scaledDpr(2, Infinity, 1), 2);
  assert.equal(scaledDpr(2, 1.5, 1), 1.5);
  assert.equal(scaledDpr(2, 1.5, 0.84), 1.26);
  assert.equal(scaledDpr(1.2, 1.5, 0.67), 1);
  assert.equal(scaledDpr(1, 1.5, 0.67), 1);
  // A sub-1 base (zoomed-out page) is kept as is.
  assert.equal(scaledDpr(0.8, 1.5, 0.67), 0.8);
});

test("adaptive quality steps down after a run of slow frames, not on one spike", () => {
  const q = new AdaptiveQuality({ levels: 3, budgetMs: 10, slowFrames: 4, fastFrames: 6 });
  for (let i = 0; i < 20; i += 1) assert.equal(q.push(i === 5 ? 40 : 3), false);
  assert.equal(q.level, 0);
  let changed = false;
  // The smoothed time needs a few frames to cross the budget.
  for (let i = 0; i < 6; i += 1) changed = q.push(25) || changed;
  assert.equal(changed, true);
  assert.equal(q.level, 1);
  // Never below the cheapest level.
  for (let i = 0; i < 40; i += 1) q.push(25);
  assert.equal(q.level, 2);
});

test("adaptive quality recovers only with wide headroom (hysteresis)", () => {
  const q = new AdaptiveQuality({ levels: 2, budgetMs: 10, slowFrames: 2, fastFrames: 5, recoverRatio: 0.5 });
  q.push(20);
  q.push(20);
  assert.equal(q.level, 1);
  // Just under budget: stays degraded.
  for (let i = 0; i < 50; i += 1) q.push(8);
  assert.equal(q.level, 1);
  for (let i = 0; i < 10; i += 1) q.push(2);
  assert.equal(q.level, 0);
  q.push(20);
  q.push(20);
  q.reset();
  assert.equal(q.level, 0);
  assert.equal(q.averageMs, -1);
  assert.equal(q.push(Number.NaN), false);
});

test("nebula cadence: interaction > beat > focus > calm twinkle", () => {
  assert.equal(nebulaLoopCadence({ active: true }).minFrameIntervalMs, NEBULA_ACTIVE_FRAME_MS);
  assert.equal(nebulaLoopCadence({ active: false, pulsing: true }).minFrameIntervalMs, NEBULA_IDLE_FRAME_MS);
  assert.equal(nebulaLoopCadence({ active: false, focused: true }).minFrameIntervalMs, NEBULA_FOCUS_FRAME_MS);
  assert.equal(nebulaLoopCadence({ active: false }).minFrameIntervalMs, NEBULA_CALM_FRAME_MS);
  assert.equal(nebulaLoopCadence({ active: false, preview: true, pulsing: true }).minFrameIntervalMs, NEBULA_PREVIEW_FRAME_MS);
  assert.ok(NEBULA_ACTIVE_FRAME_MS < NEBULA_IDLE_FRAME_MS);
  assert.ok(NEBULA_IDLE_FRAME_MS < NEBULA_FOCUS_FRAME_MS);
  assert.ok(NEBULA_FOCUS_FRAME_MS < NEBULA_CALM_FRAME_MS);
});

test("discowall: HSL conversion into a scratch array, packed RGBA words", () => {
  const out = [0, 0, 0];
  assert.equal(hslToRgbInto(0, 100, 50, out), out);
  assert.deepEqual(out, [255, 0, 0]);
  assert.deepEqual(hslToRgbInto(120, 100, 50, out), [0, 255, 0]);
  assert.deepEqual(hslToRgbInto(-120, 100, 50, out), [0, 0, 255]);
  assert.deepEqual(hslToRgbInto(200, 0, 50, out), [128, 128, 128]);
  // The word writes R, G, B, 255 into an ImageData byte buffer.
  const px = new Uint32Array(1);
  px[0] = packRgb(12, 34, 56);
  assert.deepEqual([...new Uint8Array(px.buffer)], [12, 34, 56, 255]);
});

function fakeCtx(w, h) {
  const st = { puts: 0, fills: 0, data: null };
  return {
    st,
    canvas: { width: w, height: h },
    createImageData: (a, b) => ({ width: a, height: b, data: new Uint8ClampedArray(a * b * 4) }),
    putImageData(img) {
      st.puts += 1;
      st.data = img.data;
    },
    fillRect() {
      st.fills += 1;
    },
    save() {},
    restore() {},
    setTransform() {},
  };
}

function fakeAnalyser() {
  let f = 0;
  return {
    frequencyBinCount: 1024,
    getByteFrequencyData(arr) {
      f += 1;
      const beat = f % 12 < 2 ? 1 : 0.4;
      for (let i = 0; i < arr.length; i += 1) arr[i] = Math.max(0, 255 - i * 0.4) * beat;
    },
  };
}

test("discowall renderer: one putImageData per frame, scrim folded in, opaque pixels", () => {
  const r = new DiscoWallRenderer();
  const ctx = fakeCtx(600, 240);
  const an = fakeAnalyser();
  for (let i = 0; i < 60; i += 1) {
    r.draw(ctx, { width: 600, height: 240, analyser: an, isPlaying: true, expanded: false, currentTime: 5 + i / 24, trackKey: "a/b.flac" });
  }
  assert.equal(ctx.st.puts, 60);
  // No full-canvas scrim pass any more.
  assert.equal(ctx.st.fills, 0);
  const d = ctx.st.data;
  let lit = 0;
  let maxChannel = 0;
  for (let i = 0; i < d.length; i += 4) {
    assert.equal(d[i + 3], 255);
    const m = Math.max(d[i], d[i + 1], d[i + 2]);
    if (m > 30) lit += 1;
    if (m > maxChannel) maxChannel = m;
  }
  assert.ok(lit > 0, "some cells are lit");
  // Scrim (≥ 0.28 black) keeps every channel well below full white.
  assert.ok(maxChannel < 200, `max channel ${maxChannel}`);
});
