/**
 * DiscoWall: field math (legacy discowallPlectr) + live note detection.
 *
 * Si lancia con `pnpm test` (node --experimental-strip-types).
 */
import assert from "node:assert/strict";
import { test } from "node:test";

const {
  buildPlectrFieldGrid,
  collectMotifs,
  createSceneStyleWeights,
  PLECTR_FIELD_COLS,
  PLECTR_FIELD_ROWS,
  samplePlectrFieldGrid,
} = await import("./visualizer/discowallField.ts");
const { LiveOnsetNotes, bandEnergies, lastNoteIndexAt } = await import("./visualizer/discowall.ts");

test("the field grid fills the internal grid with values in 0..1", () => {
  const styleW = createSceneStyleWeights();
  styleW.bloom = 1;
  const grid = buildPlectrFieldGrid([], styleW, 0, []);
  assert.equal(grid.field.length, PLECTR_FIELD_COLS * PLECTR_FIELD_ROWS);
  const mid = samplePlectrFieldGrid(grid, 0.5, 0.5);
  assert.ok(mid.field >= 0 && mid.field <= 1);
});

test("a tap note lights the field around its path", () => {
  const notes = [{ id: 1, type: "tap", time: 1, lane: 1, endLane: null, duration: 0 }];
  const motifs = collectMotifs(notes, 1.2, 42);
  assert.equal(motifs.length, 1);
  const grid = buildPlectrFieldGrid(motifs, createSceneStyleWeights(), 1.2, []);
  assert.ok(Math.max(...grid.field) > 0.05);
});

test("live onsets: a bass hit every 0.5 s becomes taps on lane 0 and ~120 BPM", () => {
  const live = new LiveOnsetNotes();
  const fps = 60;
  for (let f = 0; f < fps * 6; f += 1) {
    const time = f / fps;
    const phase = time % 0.5;
    const bass = phase < 0.05 ? 0.9 : 0.1;
    live.push(time, [bass, 0.1, 0.1, 0.05]);
  }
  const lane0 = live.notes.filter((n) => n.lane === 0 && n.type === "tap");
  assert.ok(lane0.length >= 4, `taps: ${lane0.length}`);
  assert.equal(live.bpm, 120);
  // Old notes are trimmed (only the last few seconds stay).
  assert.ok(live.notes.every((n) => n.time > 2));
});

test("seeking backwards resets the note stream", () => {
  const live = new LiveOnsetNotes();
  live.push(10, [0, 0, 0, 0]);
  live.push(10.02, [0.9, 0, 0, 0]);
  assert.ok(live.notes.length > 0);
  live.push(2, [0, 0, 0, 0]);
  assert.equal(live.notes.length, 0);
});

test("sustained band → hold note; band energies are averaged per band", () => {
  const live = new LiveOnsetNotes();
  for (let f = 0; f <= 60; f += 1) live.push(f / 60, [0, 0, f < 40 ? 0.8 : 0.1, 0]);
  assert.ok(live.notes.some((n) => n.type === "hold" && n.lane === 2 && n.duration >= 0.35));
  const freq = new Uint8Array(100).fill(255);
  const out = bandEnergies(freq, 100, [0, 0, 0, 0]);
  assert.deepEqual(out, [1, 1, 1, 1]);
  const notes = [{ time: 1 }, { time: 2 }, { time: 3 }];
  assert.equal(lastNoteIndexAt(notes, 2.5), 1);
  assert.equal(lastNoteIndexAt(notes, 0), -1);
});
