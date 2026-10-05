/**
 * Sonic Nebula layout math (port of legacy sonicNebula.test.ts).
 *
 * Si lancia con `pnpm test` (node --experimental-strip-types).
 */
import assert from "node:assert/strict";
import { test } from "node:test";

const {
  buildNebulaModel,
  buildNebulaSpatialGrid,
  estimateBpm,
  estimateEnergy,
  filterNebulaStars,
  nebulaStarsNear,
  nebulaWorldBounds,
  isStarInBounds,
  pickNebulaStarAt,
  sampleNebulaStarsForPreview,
  sampleTracksForNebulaBuild,
  screenToWorld,
  starCalloutLayout,
  zoomCameraAt,
  NEBULA_CENTER,
  NEBULA_GALAXY_RADIUS,
  NEBULA_MAX_ZOOM,
} = await import("./nebula/model.ts");

function track(relPath, title, moods = [], genre = "Rock") {
  return {
    relPath,
    title,
    artist: "Artist",
    album: "Album",
    durationMs: 200_000,
    genre: moods.length ? null : genre,
    moods,
  };
}

const none = { playCount: () => 0, favorites: new Set() };

test("star positions are deterministic", () => {
  const tracks = [track("a.mp3", "Alpha"), track("b.mp3", "Beta")];
  const a = buildNebulaModel(tracks, none);
  const b = buildNebulaModel(tracks, none);
  assert.equal(a.stars[0].x, b.stars[0].x);
  assert.equal(a.stars[1].y, b.stars[1].y);
});

test("caps star count keeping the most played", () => {
  const tracks = Array.from({ length: 50 }, (_, i) => track(`t-${i}.mp3`, `T${i}`));
  const model = buildNebulaModel(tracks, {
    playCount: (p) => (p === "t-42.mp3" ? 99 : 0),
    favorites: new Set(),
    maxStars: 10,
  });
  assert.equal(model.stars.length, 10);
  assert.ok(model.stars.some((s) => s.id === "t-42.mp3"));
});

test("favourites and play counts make bigger stars", () => {
  const tracks = [track("fav.mp3", "Fav")];
  const plain = buildNebulaModel(tracks, none);
  const rich = buildNebulaModel(tracks, {
    playCount: () => 32,
    favorites: new Set(["fav.mp3"]),
  });
  assert.ok(rich.stars[0].radius > plain.stars[0].radius);
  assert.equal(rich.stars[0].favorite, true);
});

test("energy follows moods, then genre; bpm uses the real value when known", () => {
  assert.ok(estimateEnergy(track("x", "x", ["aggressive_heavy"])) > 0.9);
  assert.ok(estimateEnergy(track("x", "x", ["dark_tense"])) < 0.2);
  assert.equal(estimateEnergy(track("x", "x", [], "Ambient")), 0.28);
  assert.equal(estimateBpm({ ...track("x", "x"), bpm: 128 }), 128);
  const guess = estimateBpm(track("x", "x"));
  assert.ok(guess >= 62 && guess <= 178);
});

test("filters by title/artist/album", () => {
  const model = buildNebulaModel(
    [track("1.mp3", "Energy", ["energy_boost"]), track("2.mp3", "Chill", ["chill_relax"])],
    none,
  );
  assert.equal(filterNebulaStars(model.stars, "").length, 2);
  const q = filterNebulaStars(model.stars, "chill");
  assert.equal(q.length, 1);
  assert.equal(q[0].track.title, "Chill");
});

test("covers the galaxy disk without large angular gaps", () => {
  const moods = [["dark_tense"], ["chill_relax"], ["energy_boost"]];
  const tracks = Array.from({ length: 240 }, (_, i) => track(`t${i}.mp3`, `T${i}`, moods[i % 3]));
  const model = buildNebulaModel(tracks, none);
  const sectors = Array(8).fill(0);
  const radii = [];
  for (const star of model.stars) {
    const dx = star.x - NEBULA_CENTER;
    const dy = star.y - NEBULA_CENTER;
    let angle = Math.atan2(dy, dx);
    angle = (angle + Math.PI / 2 + Math.PI * 2) % (Math.PI * 2);
    sectors[Math.floor(angle / (Math.PI / 4)) % 8] += 1;
    radii.push(Math.hypot(dx, dy) / NEBULA_GALAXY_RADIUS);
  }
  assert.ok(Math.min(...sectors) > Math.max(...sectors) * 0.18);
  assert.ok(radii.some((r) => r < 0.2));
  assert.ok(radii.some((r) => r > 0.82));
  // One fog per mood with ≥2 stars.
  assert.equal(model.fogs.length, 3);
});

test("preview sample spreads across the disk", () => {
  const model = buildNebulaModel(
    Array.from({ length: 120 }, (_, i) => track(`t${i}.mp3`, `T${i}`)),
    none,
  );
  const sample = sampleNebulaStarsForPreview(model.stars, 48);
  assert.equal(sample.length, 48);
  const oct = Array(8).fill(0);
  for (const star of sample) {
    let a = Math.atan2(star.y - NEBULA_CENTER, star.x - NEBULA_CENTER);
    a = (Math.PI / 2 - a + Math.PI * 8) % (Math.PI * 2);
    oct[Math.floor(a / (Math.PI / 4)) % 8] += 1;
  }
  assert.ok(Math.min(...oct) > 0);
  assert.equal(sampleTracksForNebulaBuild([1, 2, 3, 4, 5, 6], 3).length, 3);
});

test("neighbours and hit-testing", () => {
  const model = buildNebulaModel(
    Array.from({ length: 12 }, (_, i) => track(`t${i}.mp3`, `T${i}`)),
    none,
  );
  const center = model.stars[0];
  assert.ok(nebulaStarsNear(model.stars, center, 320, 5).length > 0);
  const grid = buildNebulaSpatialGrid(model.stars);
  assert.equal(pickNebulaStarAt(grid, center.x, center.y, 1)?.id, center.id);
  assert.equal(pickNebulaStarAt(grid, -5000, -5000, 1), null);
});

test("camera: zoom keeps the point under the cursor, bounds cull", () => {
  const rect = { left: 0, top: 0, width: 800, height: 600 };
  const cam = { x: 1000, y: 1000, zoom: 1 };
  const before = screenToWorld(600, 200, rect, cam);
  const next = zoomCameraAt(rect, 600, 200, cam, 2);
  const after = screenToWorld(600, 200, rect, next);
  assert.ok(Math.abs(before.wx - after.wx) < 1e-9);
  assert.ok(Math.abs(before.wy - after.wy) < 1e-9);
  assert.equal(zoomCameraAt(rect, 0, 0, cam, 99).zoom, NEBULA_MAX_ZOOM);
  const b = nebulaWorldBounds(800, 600, cam);
  assert.equal(isStarInBounds({ x: 1000, y: 1000, radius: 4 }, b), true);
  assert.equal(isStarInBounds({ x: 3000, y: 1000, radius: 4 }, b), false);
});

test("callout stays inside the stage", () => {
  const right = starCalloutLayout(790, 300, 800, 600);
  assert.ok(right.left + 252 <= 800);
  const top = starCalloutLayout(100, 0, 800, 600);
  assert.equal(top.top, 10);
});

test("10k tracks build in reasonable time", () => {
  const tracks = Array.from({ length: 10_000 }, (_, i) => track(`a${i % 97}/b${i}.flac`, `T${i}`));
  const t0 = performance.now();
  const model = buildNebulaModel(tracks, none);
  const ms = performance.now() - t0;
  assert.ok(model.stars.length <= 8000);
  assert.ok(ms < 3000, `build took ${ms}ms`);
});

test("painter runs on a fake 2D context (culling + batching, no DOM)", async () => {
  const { paintNebulaFrame } = await import("./nebula/render.ts");
  const calls = { arc: 0, fill: 0 };
  const gradient = { addColorStop() {} };
  const ctx = new Proxy(
    {},
    {
      get(_t, prop) {
        if (prop === "createRadialGradient" || prop === "createLinearGradient") return () => gradient;
        if (prop === "arc") return () => (calls.arc += 1);
        if (prop === "fill") return () => (calls.fill += 1);
        return () => {};
      },
      set() {
        return true;
      },
    },
  );
  const tracks = Array.from({ length: 3000 }, (_, i) =>
    track(`x/${i}.mp3`, `T${i}`, i % 2 ? ["chill_relax"] : ["party_dance"]),
  );
  const model = buildNebulaModel(tracks, none);
  const sorted = [...model.stars].sort((a, b) => a.radius - b.radius);
  const base = {
    fogs: model.fogs,
    sortedStars: sorted,
    hoveredId: null,
    selectedId: sorted[10].id,
    currentId: sorted[20].id,
    playing: true,
    currentBpm: 120,
    beatEpoch: 0,
    preview: false,
    reducedMotion: false,
  };
  paintNebulaFrame(ctx, 800, 600, 1000, { ...base, camera: { x: 1100, y: 1100, zoom: 0.52 } });
  const wideFills = calls.fill;
  // Small stars are batched per colour: far fewer fills than stars.
  assert.ok(wideFills < 200, `fills: ${wideFills}`);
  calls.arc = 0;
  paintNebulaFrame(ctx, 800, 600, 1000, { ...base, camera: { x: 1100, y: 1100, zoom: 2.8 } });
  // Zoomed in: most stars are culled.
  assert.ok(calls.arc < 3000, `arcs: ${calls.arc}`);
});

test("source: tag BPM wins over chart BPM, chart over the guess", async () => {
  const { nebulaInputsFromTracks } = await import("./nebula/source.ts");
  const base = { id: 1, title: "T", artist_name: "A", album_name: "B", duration_ms: 1, track_number: null, album_id: null, artist_id: null };
  const tracks = [
    { ...base, rel_path: "tag.mp3", bpm: 128 },
    { ...base, rel_path: "chart.mp3" },
    { ...base, rel_path: "none.mp3" },
  ];
  const fromChart = (p) => (p === "chart.mp3" || p === "tag.mp3" ? 90 : null);
  const inputs = nebulaInputsFromTracks(tracks, {}, fromChart);
  assert.deepEqual(inputs.map((i) => i.bpm), [128, 90, null]);
  assert.equal(estimateBpm(inputs[1]), 90);
});
