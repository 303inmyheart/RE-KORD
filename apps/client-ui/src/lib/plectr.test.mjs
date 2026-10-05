/**
 * Plectr: chart sanitizing, difficulty migration, records, smooth song clock,
 * run timing, chart loader races, judging engine, chart generation and the
 * badge i18n tables. Ported from the legacy vitest suites in `src/game/**`
 * (chartSanitize, plectrDifficultyStorage, plectrStorage, smoothSongClock,
 * GameCanvas.sync, useRhythmChart) plus engine / analysis checks.
 * Si lancia con `pnpm test` (node --experimental-strip-types).
 */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test, describe } from "node:test";

const store = new Map();
globalThis.localStorage = {
  getItem: (k) => (store.has(k) ? store.get(k) : null),
  setItem: (k, v) => store.set(k, String(v)),
  removeItem: (k) => store.delete(k),
  clear: () => store.clear(),
  key: (i) => [...store.keys()][i] ?? null,
  get length() {
    return store.size;
  },
};

const { sanitizeChartForKord, sanitizeChartSetForRekord } = await import("./plectr/chartSanitize.ts");
const { migratePlectrPlayMode, readLegacyPlectrPlayMode, LEGACY_PLECTR_DIFFICULTY_KEY } =
  await import("./plectr/difficulty.ts");
const records = await import("./plectr/records.ts");
const { buildGameResult, resolveRunEndTime, resultGrade } = await import("./plectr/runResult.ts");
const { createSongClockState, resetSongClock, resolveSmoothSongTime } = await import(
  "./plectr/smoothSongClock.ts"
);
const { ChartLoader } = await import("./plectr/chartLoader.ts");
const engine = await import("./plectr/engine.ts");
const { analyzeLibraryBuffer } = await import("./plectr/audioAnalysis.ts");
const { DIFFICULTIES, HIT_WINDOWS, LANES } = await import("./plectr/config.ts");
const { ACHIEVEMENT_IDS, buildAchievementsSnapshot } = await import("./achievements.ts");

const difficulty = DIFFICULTIES[0];

function note(id, time, lane, extra = {}) {
  return {
    id,
    type: "tap",
    direction: null,
    time,
    lane,
    endLane: null,
    duration: 0,
    hit: false,
    missed: false,
    holding: false,
    completed: false,
    ...extra,
  };
}

function chartWith(notes) {
  return {
    songId: "song:easy",
    baseSongId: "song",
    difficulty,
    title: "Song",
    duration: 30,
    notes,
    stats: { bpm: 120, rmsAvg: 0.1, density: 1 },
  };
}

describe("sanitizeChartForKord", () => {
  test("keeps maintained notes on every difficulty and removes swipes", () => {
    const sanitized = sanitizeChartForKord(
      chartWith([
        note(0, 4, 1, { endLane: 1, duration: 0.8, hit: true, missed: true, holding: true, completed: true }),
        note(1, 5, 2, { type: "swipe", direction: "up" }),
      ]),
    );
    assert.equal(sanitized.notes.length, 1);
    assert.deepEqual(
      {
        id: sanitized.notes[0].id,
        type: sanitized.notes[0].type,
        direction: sanitized.notes[0].direction,
        duration: sanitized.notes[0].duration,
        hit: sanitized.notes[0].hit,
        missed: sanitized.notes[0].missed,
        holding: sanitized.notes[0].holding,
        completed: sanitized.notes[0].completed,
      },
      {
        id: 0,
        type: "hold",
        direction: null,
        duration: 0.8,
        hit: false,
        missed: false,
        holding: false,
        completed: false,
      },
    );
  });

  test("sanitizes every difficulty of a chart set", () => {
    const set = sanitizeChartSetForRekord({
      baseSongId: "s",
      title: "s",
      duration: 30,
      charts: { easy: chartWith([note(0, 4, 0, { type: "swipe" })]), normal: chartWith([note(0, 4, 0)]), hard: chartWith([]) },
    });
    assert.equal(set.charts.easy.notes.length, 0);
    assert.equal(set.charts.normal.notes.length, 1);
  });
});

describe("difficulty", () => {
  test("keeps current difficulty ids and migrates extreme to hard", () => {
    assert.equal(migratePlectrPlayMode("easy"), "easy");
    assert.equal(migratePlectrPlayMode("normal"), "normal");
    assert.equal(migratePlectrPlayMode("hard"), "hard");
    assert.equal(migratePlectrPlayMode("extreme"), "hard");
  });

  test("defaults unknown values to easy", () => {
    assert.equal(migratePlectrPlayMode(null), "easy");
    assert.equal(migratePlectrPlayMode("insane"), "easy");
  });

  test("reads the legacy client's saved difficulty", () => {
    store.clear();
    assert.equal(readLegacyPlectrPlayMode(), null);
    localStorage.setItem(LEGACY_PLECTR_DIFFICULTY_KEY, "extreme");
    assert.equal(readLegacyPlectrPlayMode(), "hard");
    store.clear();
  });
});

describe("records", () => {
  test("counts distinct saved records", () => {
    assert.equal(
      records.countPlectrTracksPlayed({
        "a.mp3": { score: 1200, grade: "A", accuracy: 0.9, maxCombo: 10, hits: 20 },
        "b.mp3": { score: 0, grade: "", accuracy: 0, maxCombo: 0, hits: 0 },
      }),
      1,
    );
  });

  test("picks better score by points then accuracy", () => {
    const low = buildGameResult({ score: 100, maxCombo: 1, hits: 5, misses: 1 });
    const high = buildGameResult({ score: 200, maxCombo: 2, hits: 8, misses: 0 });
    assert.deepEqual(records.pickBetterPlectrScore(low, high), high);
    assert.equal(records.isBetterPlectrScore(high, low), true);
    const sameScoreBetterAcc = buildGameResult({ score: 100, maxCombo: 1, hits: 6, misses: 0 });
    assert.equal(records.isBetterPlectrScore(sameScoreBetterAcc, low), true);
  });

  test("requires positive score or hits for a play record", () => {
    assert.equal(records.hasPlectrPlayRecord({ score: 0, grade: "", accuracy: 0, maxCombo: 0, hits: 0 }), false);
    assert.equal(records.hasPlectrPlayRecord({ score: 0, grade: "", accuracy: 0, maxCombo: 0, hits: 3 }), true);
  });

  test("persists when session already matches but account has no record", () => {
    const relPath = "song.mp3";
    const session = new records.SessionBests();
    const result = buildGameResult({ score: 500, maxCombo: 5, hits: 10, misses: 1 });
    session.save(relPath, result);
    const accountBest = records.plectrBestFromBests(undefined, relPath);
    assert.equal(records.isBetterPlectrScore(result, accountBest), true);
    assert.equal(
      records.isBetterPlectrScore(result, records.pickBetterPlectrScore(session.get(relPath), accountBest)),
      false,
    );
  });

  test("looks records up across the Tracce/Tracks loose-folder rename", () => {
    const bests = { "Artist/Tracce/a.mp3": { score: 900, grade: "B", accuracy: 0.8, maxCombo: 4, hits: 8 } };
    assert.equal(records.plectrBestFromBests(bests, "Artist/Tracks/a.mp3")?.score, 900);
  });

  test("applyRunToStore keeps one best per difficulty and counts runs", () => {
    let s = records.emptyPlectrStore();
    const first = buildGameResult({ score: 1000, maxCombo: 10, hits: 20, misses: 5 });
    let out = records.applyRunToStore(s, "a.mp3", first, "normal", new Date("2026-01-01T00:00:00Z"));
    assert.equal(out.newRecord, true);
    s = out.store;
    assert.equal(s.runs, 1);
    assert.equal(s.notesHit, 20);
    assert.equal(s.bests["a.mp3"].difficulty, "normal");
    assert.equal(s.byDifficulty["a.mp3"].normal.score, 1000);
    // Another difficulty has its own slot: a lower score there is still its record.
    const hard = buildGameResult({ score: 400, maxCombo: 3, hits: 9, misses: 9 });
    out = records.applyRunToStore(s, "a.mp3", hard, "hard");
    assert.equal(out.newRecord, true);
    assert.equal(out.previous, null);
    assert.equal(out.store.byDifficulty["a.mp3"].hard.score, 400);
    assert.equal(out.store.bests["a.mp3"].score, 1000, "overall best unchanged");
    assert.equal(out.store.runs, 2);
    // Worse on the same difficulty: no record, previous reported.
    const worse = buildGameResult({ score: 300, maxCombo: 3, hits: 9, misses: 9 });
    const again = records.applyRunToStore(out.store, "a.mp3", worse, "hard");
    assert.equal(again.newRecord, false);
    assert.equal(again.previous?.score, 400);
    assert.equal(again.store.byDifficulty["a.mp3"].hard.score, 400);
    const empty = buildGameResult({ score: 0, maxCombo: 0, hits: 0, misses: 0 });
    assert.equal(records.applyRunToStore(out.store, "a.mp3", empty, "easy").store, out.store);
  });

  test("runs that do not count change nothing", () => {
    const s = records.emptyPlectrStore();
    const result = buildGameResult({ score: 9000, maxCombo: 40, hits: 50, misses: 0 });
    const out = records.applyRunToStore(s, "a.mp3", result, "easy", new Date(), { eligible: false });
    assert.equal(out.store, s);
    assert.equal(out.newRecord, false);
    assert.equal(out.store.runs, 0);
  });

  test("FC / AP flags stick to the slot and full runs are counted", () => {
    let s = records.emptyPlectrStore();
    const fc = buildGameResult({ score: 500, maxCombo: 20, hits: 20, misses: 0 });
    s = records.applyRunToStore(s, "a.mp3", fc, "easy", new Date("2026-01-01T00:00:00Z"), {
      fullRun: true,
      fc: true,
    }).store;
    const better = buildGameResult({ score: 800, maxCombo: 10, hits: 25, misses: 2 });
    s = records.applyRunToStore(s, "a.mp3", better, "easy", new Date("2026-01-02T00:00:00Z")).store;
    assert.equal(s.byDifficulty["a.mp3"].easy.score, 800);
    assert.equal(s.byDifficulty["a.mp3"].easy.fc, true);
    assert.equal(s.fullRuns, 1);
  });

  test("v1 single best migrates to its difficulty slot", () => {
    const v1 = {
      version: 1,
      difficulty: "normal",
      bests: {
        "tagged.mp3": { score: 700, grade: "A", accuracy: 0.92, maxCombo: 30, hits: 46, misses: 4, difficulty: "hard", updatedAt: "2026-01-01T00:00:00.000Z" },
        "legacy.mp3": { score: 300, grade: "C", accuracy: 0.71, maxCombo: 8, hits: 20, misses: 8 },
      },
      runs: 4,
      notesHit: 66,
      lowEnd: true,
    };
    const s = records.normalizePlectrStore(v1);
    assert.equal(s.version, 2);
    assert.equal(s.byDifficulty["tagged.mp3"].hard.score, 700);
    assert.equal(s.byDifficulty["legacy.mp3"], undefined, "untagged legacy records stay overall-only");
    assert.equal(s.bests["legacy.mp3"].score, 300);
    assert.equal(s.settings.lightStage, "on", "v1 light-stage flag seeds the setting");
    // Selectors see both.
    const rows = records.selectPlectrTrackRecords(s);
    assert.deepEqual(rows.map((r) => r.relPath).sort(), ["legacy.mp3", "tagged.mp3"]);
    assert.equal(records.selectPlectrTrackRecord(s, "tagged.mp3")?.byDifficulty.hard?.grade, "A");
    const career = records.selectPlectrCareer(s);
    assert.equal(career.tracksPlayed, 2);
    assert.equal(career.grades.A, 1);
    assert.equal(career.grades.C, 1);
    assert.ok(career.xp > 0);
  });

  test("settings normalize: ranges, keys, defaults", () => {
    const st = records.normalizePlectrSettings({ speed: 9, latencyMs: -999, keys: ["a", "a", "b", "c"], backdrop: "nope" });
    assert.equal(st.speed, 1.6);
    assert.equal(st.latencyMs, -150);
    assert.deepEqual(st.keys, ["d", "f", "j", "k"], "duplicate keys fall back to D F J K");
    assert.equal(st.backdrop, "art");
    const arrows = records.normalizePlectrSettings({ keys: ["ArrowLeft", "ArrowDown", "ArrowUp", "ArrowRight"] });
    assert.deepEqual(arrows.keys, ["arrowleft", "arrowdown", "arrowup", "arrowright"]);
  });

  test("recent list: newest first, unique, capped", () => {
    let s = records.emptyPlectrStore();
    for (let i = 0; i < 30; i += 1) s = records.touchRecent(s, `t${i}.mp3`);
    s = records.touchRecent(s, "t10.mp3");
    assert.equal(s.recent[0], "t10.mp3");
    assert.equal(s.recent.length, records.RECENT_LIMIT);
    assert.equal(new Set(s.recent).size, s.recent.length);
  });

  test("merging stores never loses a record, reset drops older ones", () => {
    const a = records.normalizePlectrStore({
      difficulty: "hard",
      bests: { "a.mp3": { score: 10, grade: "D", accuracy: 0.5, maxCombo: 1, hits: 1, updatedAt: "2026-01-01T00:00:00.000Z" } },
      runs: 3,
    });
    const b = records.normalizePlectrStore({
      bests: {
        "a.mp3": { score: 50, grade: "C", accuracy: 0.7, maxCombo: 2, hits: 4, updatedAt: "2026-01-02T00:00:00.000Z" },
        "b.mp3": { score: 70, grade: "B", accuracy: 0.8, maxCombo: 3, hits: 5, updatedAt: "2026-01-03T00:00:00.000Z" },
        junk: { score: "nope" },
      },
      runs: 5,
    });
    const merged = records.mergePlectrStores(a, b);
    assert.equal(merged.difficulty, "hard");
    assert.equal(merged.bests["a.mp3"].score, 50);
    assert.equal(merged.bests["b.mp3"].score, 70);
    assert.equal("junk" in merged.bests, false);
    assert.equal(merged.runs, 5);

    const reset = records.resetPlectrStore(merged, new Date("2026-01-02T12:00:00.000Z"));
    // The stale copy on another device still has both records.
    const afterReset = records.mergePlectrStores(reset, b);
    assert.deepEqual(Object.keys(afterReset.bests), ["b.mp3"]);
    assert.equal(afterReset.runs, 0);
  });

  test("normalizePlectrStore accepts garbage", () => {
    const s = records.normalizePlectrStore("{oops");
    assert.equal(s.difficulty, "easy");
    assert.deepEqual(s.bests, {});
    assert.equal(records.normalizePlectrStore({ difficulty: "extreme" }).difficulty, "hard");
  });
});

describe("run result", () => {
  test("grades by accuracy", () => {
    assert.equal(resultGrade(0.96, false), "S");
    assert.equal(resultGrade(0.91, false), "A");
    assert.equal(resultGrade(0.5, false), "D");
    assert.equal(resultGrade(1, true), "F");
  });

  test("uses audio duration when available", () => {
    assert.equal(resolveRunEndTime(240, 238.5), 238.5);
  });

  test("falls back to chart duration", () => {
    assert.equal(resolveRunEndTime(240, undefined), 240);
    assert.equal(resolveRunEndTime(240, NaN), 240);
  });
});

function makeBridge(reportedTime, playing = true) {
  const audio = playing
    ? { currentTime: reportedTime, paused: false, ended: false, playbackRate: 1 }
    : null;
  return {
    getCurrentTime: () => reportedTime,
    getAudio: () => audio,
  };
}

describe("resolveSmoothSongTime", () => {
  test("advances smoothly while audio.currentTime is frozen between samples", () => {
    const clock = createSongClockState();
    const bridge = makeBridge(10);
    resetSongClock(clock, 10, 1000);
    clock.audioSampleSong = 10;
    clock.audioSamplePerf = 1000;
    clock.audioPlaybackRate = 1;

    let songTime = 10;
    for (let perf = 1016; perf <= 5000; perf += 16) {
      songTime = resolveSmoothSongTime(clock, perf, bridge);
    }
    assert.ok(songTime > 13.5, `got ${songTime}`);
    assert.ok(songTime < 14.2, `got ${songTime}`);

    songTime = resolveSmoothSongTime(clock, 5016, makeBridge(14));
    assert.ok(songTime > 13.8, `got ${songTime}`);
    assert.ok(songTime < 14.3, `got ${songTime}`);
  });

  test("stops when the player is paused", () => {
    const clock = createSongClockState();
    const songTime = resolveSmoothSongTime(clock, 2000, makeBridge(42, false));
    assert.equal(songTime, 42);
    assert.equal(clock.clockAnchorSong, 42);
  });

  test("snaps to the player after a seek", () => {
    const clock = createSongClockState();
    resolveSmoothSongTime(clock, 1000, makeBridge(10));
    resolveSmoothSongTime(clock, 1016, makeBridge(10));
    const t = resolveSmoothSongTime(clock, 1032, makeBridge(80));
    assert.equal(t, 80);
  });
});

/* ── Chart loader (legacy useRhythmChart races) ── */

const tick = () => new Promise((r) => setTimeout(r, 0));

function chartSet(id) {
  return { baseSongId: id, title: id, duration: 60, charts: {} };
}

describe("ChartLoader", () => {
  test("restarts load after abort when the track changes (no stuck loading)", async () => {
    const calls = [];
    const states = [];
    const loader = new ChartLoader(
      {
        peekCached: () => null,
        analyze: (track, _progress, signal) => {
          calls.push({ relPath: track.rel_path, signal });
          if (track.rel_path === "a.mp3") return new Promise(() => {});
          return Promise.resolve(chartSet("b"));
        },
      },
      (s) => states.push(s),
    );
    loader.load({ rel_path: "a.mp3" });
    assert.equal(loader.state.phase, "loading");
    loader.load({ rel_path: "b.mp3" });
    assert.equal(calls.length, 2);
    assert.equal(calls[0].signal.aborted, true);
    assert.equal(calls[1].relPath, "b.mp3");
    await tick();
    assert.equal(loader.state.phase, "ready");
    assert.equal(loader.state.relPath, "b.mp3");
    assert.equal(loader.state.chartSet?.baseSongId, "b");
  });

  test("ignores a stale async result after a fast return to a cached track", async () => {
    let resolveB = () => {};
    const bPromise = new Promise((resolve) => {
      resolveB = resolve;
    });
    const loader = new ChartLoader(
      {
        peekCached: (relPath) => (relPath === "a.mp3" ? chartSet("a") : null),
        analyze: () => bPromise,
      },
      () => {},
    );
    loader.load({ rel_path: "a.mp3" });
    assert.equal(loader.state.phase, "ready");
    assert.equal(loader.state.chartSet?.baseSongId, "a");

    loader.load({ rel_path: "b.mp3" });
    assert.equal(loader.state.phase, "loading");

    loader.load({ rel_path: "a.mp3" });
    assert.equal(loader.state.phase, "ready");
    assert.equal(loader.state.chartSet?.baseSongId, "a");

    resolveB(chartSet("b"));
    await tick();
    assert.equal(loader.state.chartSet?.baseSongId, "a");
    assert.equal(loader.state.relPath, "a.mp3");
  });

  test("returns to the same loading track without restarting it", () => {
    let calls = 0;
    const loader = new ChartLoader(
      { peekCached: () => null, analyze: () => (calls++, new Promise(() => {})) },
      () => {},
    );
    loader.load({ rel_path: "a.mp3" });
    loader.load({ rel_path: "a.mp3" });
    assert.equal(calls, 1);
    loader.destroy();
  });

  test("reports the analysis error code and retries on force", async () => {
    let fail = true;
    const loader = new ChartLoader(
      {
        peekCached: () => null,
        analyze: () => (fail ? Promise.reject(new Error("x")) : Promise.resolve(chartSet("a"))),
        errorCodeOf: () => "sparse",
      },
      () => {},
    );
    loader.load({ rel_path: "a.mp3" });
    await tick();
    assert.equal(loader.state.phase, "error");
    assert.equal(loader.state.errorCode, "sparse");
    fail = false;
    loader.load({ rel_path: "a.mp3" });
    assert.equal(loader.state.phase, "error");
    loader.load({ rel_path: "a.mp3" }, true);
    await tick();
    assert.equal(loader.state.phase, "ready");
  });

  test("goes idle with no track", () => {
    const loader = new ChartLoader({ peekCached: () => chartSet("a"), analyze: () => Promise.reject() }, () => {});
    loader.load({ rel_path: "a.mp3" });
    loader.load(null);
    assert.equal(loader.state.phase, "idle");
    assert.equal(loader.state.chartSet, null);
  });
});

/* ── Judging engine ── */

describe("engine", () => {
  const env = (now = 0) => ({ now, onMiss: () => {} });

  test("tap judging: perfect / good / early-late / outside the window", () => {
    const run = engine.initialRunState([note(0, 5, 0), note(1, 6, 1), note(2, 7, 2), note(3, 8, 3)]);
    run.started = true;
    run.songTime = 5.01;
    engine.pressLane(run, 0, env());
    assert.equal(run.feedback, "perfect");
    assert.equal(run.score, 300);
    run.songTime = 6.09;
    engine.pressLane(run, 1, env());
    assert.equal(run.feedback, "good");
    run.songTime = 6.87;
    engine.pressLane(run, 2, env());
    assert.equal(run.feedback, "early");
    run.songTime = 7.5;
    assert.equal(engine.judgeTap(run, 3, env()), false);
    assert.equal(run.hits, 3);
    assert.equal(run.combo, 3);
    assert.equal(run.perfects, 1);
    assert.equal(run.goods, 1);
    assert.equal(run.oks, 1);
  });

  test("combo multiplier grows every 12 hits up to x4", () => {
    const notes = Array.from({ length: 60 }, (_, i) => note(i, 5 + i * 0.5, i % 4));
    const run = engine.initialRunState(notes);
    for (const n of notes) {
      run.songTime = n.time;
      engine.pressLane(run, n.lane, env());
      engine.releaseLane(run, n.lane, env());
    }
    assert.equal(run.maxCombo, 60);
    // 11×300×1 + 12×300×2 + 12×300×3 + 25×300×4
    assert.equal(run.score, 11 * 300 + 12 * 600 + 12 * 900 + 25 * 1200);
  });

  test("notes that scroll past are misses and break the combo", () => {
    let misses = 0;
    const run = engine.initialRunState([note(0, 5, 0), note(1, 6, 1)]);
    run.songTime = 5;
    engine.pressLane(run, 0, env());
    assert.equal(run.combo, 1);
    engine.applyMisses(run, 6 + HIT_WINDOWS.ok + 0.01, { now: 0, onMiss: () => misses++ });
    assert.equal(run.misses, 1);
    assert.equal(run.combo, 0);
    assert.equal(run.feedback, "miss");
    assert.equal(misses, 1);
    assert.equal(engine.runResultOf(run).accuracy, 0.5);
  });

  test("hold: kept to the tail completes, released early is a hold miss", () => {
    const hold = (id, t, lane) => note(id, t, lane, { type: "hold", duration: 1, endLane: lane });
    const run = engine.initialRunState([hold(0, 5, 0), hold(1, 8, 1)]);
    run.songTime = 5;
    engine.pressLane(run, 0, env());
    assert.equal(run.activeHolds.length, 1);
    engine.completeHeldNotes(run, 5.5, env());
    assert.equal(run.activeHolds.length, 1);
    engine.completeHeldNotes(run, 6.01, env());
    assert.equal(run.activeHolds.length, 0);
    assert.equal(run.notes[0].completed, true);
    engine.releaseLane(run, 0, env());
    assert.equal(run.misses, 0);

    run.songTime = 8;
    engine.pressLane(run, 1, env());
    run.songTime = 8.2;
    engine.releaseLane(run, 1, env());
    assert.equal(run.feedback, "holdMiss");
    assert.equal(run.misses, 1);
    assert.equal(run.combo, 0);
  });

  test("joining mid-track skips what is behind the line without misses", () => {
    const run = engine.initialRunState([note(0, 5, 0), note(1, 6, 1), note(2, 30, 2)]);
    engine.skipNotesBefore(run, 20);
    engine.applyMisses(run, 20, env());
    assert.equal(run.misses, 0);
    assert.equal(run.hits, 0);
    assert.equal(engine.isChartRunComplete(run, 20), false);
    run.songTime = 30;
    engine.pressLane(run, 2, env());
    assert.equal(engine.isChartRunComplete(run, 30 + HIT_WINDOWS.ok), true);
  });

  test("lane flashes expire", () => {
    const run = engine.initialRunState([note(0, 5, 0)]);
    run.songTime = 5;
    engine.pressLane(run, 0, env(1000));
    assert.equal(engine.activeFlash(run, 0, 1100), "hit");
    assert.equal(engine.activeFlash(run, 0, 2000), null);
  });
});

/* ── Chart generation ── */

describe("audio analysis", () => {
  test("builds playable charts for every difficulty from a beat", async () => {
    const sampleRate = 22050;
    const duration = 40;
    const length = sampleRate * duration;
    const data = new Float32Array(length);
    const bpm = 120;
    const beat = 60 / bpm;
    // Quiet bed + decaying clicks on the beat, louder on the downbeat.
    for (let i = 0; i < length; i += 1) data[i] = Math.sin(i * 0.05) * 0.01;
    for (let b = 0; b * beat < duration; b += 1) {
      const start = Math.floor(b * beat * sampleRate);
      const amp = b % 4 === 0 ? 0.9 : 0.5;
      for (let j = 0; j < 2200 && start + j < length; j += 1) {
        data[start + j] += amp * Math.exp(-j / 300) * Math.sin(j * (b % 2 ? 0.9 : 0.3));
      }
    }
    const audio = {
      length,
      numberOfChannels: 1,
      sampleRate,
      duration,
      getChannelData: () => data,
    };
    const set = sanitizeChartSetForRekord(await analyzeLibraryBuffer(audio, "Artist/Album/beat.mp3", "Beat"));
    assert.equal(set.baseSongId, "rekord:Artist/Album/beat.mp3:40000");
    for (const d of DIFFICULTIES) {
      const chart = set.charts[d.id];
      assert.ok(chart.notes.length >= 12, `${d.id}: ${chart.notes.length} notes`);
      let prev = -Infinity;
      for (const n of chart.notes) {
        assert.ok(n.time >= prev, "notes sorted by time");
        prev = n.time;
        assert.ok(n.time >= 4 && n.time < duration, `note in range: ${n.time}`);
        assert.ok(n.lane >= 0 && n.lane < LANES.length);
        assert.ok(n.type === "tap" || n.type === "hold");
        if (n.type === "hold") assert.ok(n.duration > 0);
      }
      assert.ok(chart.stats.bpm >= 78 && chart.stats.bpm <= 176);
    }
    assert.ok(set.charts.hard.notes.length >= set.charts.easy.notes.length);
  });
});

/* ── Achievements i18n ── */

describe("achievement badges", () => {
  const en = JSON.parse(readFileSync(new URL("../locales/plectr/en.json", import.meta.url), "utf8"));
  const it = JSON.parse(readFileSync(new URL("../locales/plectr/it.json", import.meta.url), "utf8"));

  test("every badge has an English and Italian title and description", () => {
    assert.equal(ACHIEVEMENT_IDS.length, 65);
    for (const id of ACHIEVEMENT_IDS) {
      for (const table of [en, it]) {
        assert.ok(table[`achievements.badge.${id}.title`], `title ${id}`);
        assert.ok(table[`achievements.badge.${id}.desc`], `desc ${id}`);
      }
    }
  });

  test("Plectr badges unlock from the Plectr track counter", () => {
    const snap = (plectrTracksPlayed) =>
      buildAchievementsSnapshot({
        playCounts: {},
        tracks: [],
        favoritesCount: 0,
        playlistsCount: 0,
        playlistTrackCount: 0,
        libraryTrackCount: 0,
        shuffleBlocks: 0,
        genreForTrack: () => null,
        plectrTracksPlayed,
      }).achievements.filter((a) => a.id.startsWith("plectr_") && a.unlocked).length;
    assert.equal(snap(0), 0);
    assert.equal(snap(10), 1);
    assert.equal(snap(120), 3);
    assert.equal(snap(500), 5);
  });

  test("Plectr i18n tables have the same keys", () => {
    assert.deepEqual(Object.keys(en).sort(), Object.keys(it).sort());
  });
});

const timing = await import("./plectr/timing.ts");
const { GRACE_SECONDS, BASE_LEAD_TIME } = await import("./plectr/config.ts");

describe("grace period", () => {
  const env = { now: 0 };
  test("no miss on notes due within the grace window after joining", () => {
    const s = engine.initialRunState([note(0, 10, 0), note(1, 10.8, 1), note(2, 11.4, 2), note(3, 12, 3)]);
    engine.skipNotesBefore(s, 10.2);
    engine.startGrace(s, 10.2, GRACE_SECONDS);
    engine.applyMisses(s, 13, env);
    // 10 was behind the join point, 10.8 / 11.4 fall inside the grace: skipped.
    assert.equal(s.misses, 1, "only the note after the grace window is missed");
    assert.equal(s.skipped, 3);
    assert.equal(s.jumped, 1, "only the note behind the join point was jumped over");
    assert.equal(engine.judgedNotes(s), 1);
  });

  test("notes left in the resume grace are skipped, not jumped over", () => {
    const s = engine.initialRunState([note(0, 1, 0), note(1, 20.5, 1), note(2, 25, 2)]);
    s.songTime = 1.01;
    engine.pressLane(s, 0, env);
    engine.releaseLane(s, 0, env);
    // Pause at 20 s, resume: the next note falls in the grace and is not played.
    engine.startGrace(s, 20, GRACE_SECONDS);
    engine.applyMisses(s, 22, env);
    assert.equal(s.skipped, 1);
    assert.equal(s.jumped, 0);
    const run = { reason: "end", fromStart: true, skipped: s.skipped, jumped: s.jumped, judged: 2, totalNotes: 3 };
    assert.equal(timing.isRecordEligible(run), true, "a paused run from the top is a full run");
  });

  test("hits inside the grace window still count", () => {
    const s = engine.initialRunState([note(0, 5, 0)]);
    engine.startGrace(s, 4.5, GRACE_SECONDS);
    s.songTime = 5.01;
    engine.pressLane(s, 0, env);
    assert.equal(s.hits, 1);
    assert.equal(s.skipped, 0);
  });

  test("resume grace extends, never shortens", () => {
    const s = engine.initialRunState([]);
    engine.startGrace(s, 20, GRACE_SECONDS);
    engine.startGrace(s, 5, GRACE_SECONDS);
    assert.equal(s.graceUntil, 20 + GRACE_SECONDS);
  });

  test("hit offsets, early/late split and combo milestones", () => {
    const notes = Array.from({ length: 26 }, (_, i) => note(i, 1 + i * 0.5, i % 4));
    const s = engine.initialRunState(notes);
    let milestones = 0;
    const e = { now: 0, onMilestone: () => (milestones += 1) };
    for (const n of notes) {
      s.songTime = n.time + 0.12; // late "ok"
      engine.pressLane(s, n.lane, e);
      engine.releaseLane(s, n.lane, e);
    }
    assert.equal(s.lates, 26);
    assert.equal(s.earlies, 0);
    assert.equal(milestones, 1, "combo 25");
    assert.ok(s.offsets.length <= 24 && s.offsets.every((o) => o === 120));
    assert.equal(engine.comboMultiplier(26), 3);
  });
});

describe("record eligibility", () => {
  const base = { reason: "end", fromStart: true, skipped: 0, judged: 100, totalNotes: 100 };
  test("full run counts", () => assert.equal(timing.isRecordEligible(base), true));
  test("interrupted never counts", () =>
    assert.equal(timing.isRecordEligible({ ...base, reason: "interrupted" }), false));
  test("failed challenge never counts", () =>
    assert.equal(timing.isRecordEligible({ ...base, failed: true }), false));
  test("joined late: needs 60% judged", () => {
    assert.equal(timing.isRecordEligible({ ...base, fromStart: false, skipped: 41, judged: 59 }), false);
    assert.equal(timing.isRecordEligible({ ...base, fromStart: false, skipped: 40, judged: 60 }), true);
  });
  test("nothing judged never counts", () =>
    assert.equal(timing.isRecordEligible({ ...base, judged: 0 }), false));
});

describe("countdown timing", () => {
  test("one beat per step, folded into 0.5-0.8 s", () => {
    assert.equal(timing.countdownStepSeconds(120), 0.5);
    assert.ok(Math.abs(timing.countdownStepSeconds(90) - 2 / 3) < 1e-9);
    assert.equal(timing.countdownStepSeconds(160), 0.75, "fast tempo: two beats per step");
    assert.equal(timing.countdownStepSeconds(60), 0.5, "slow tempo: half a beat");
    assert.equal(timing.countdownStepSeconds(NaN), 0.6);
    assert.equal(timing.countdownStepSeconds(40), 0.75);
  });

  test("3-2-1-VIA with a 2 s lead-in, audio on VIA", () => {
    const plan = timing.countdownPlan(120);
    assert.equal(plan.total, 2);
    assert.equal(timing.countdownLabelAt(plan, 0.2), null);
    assert.equal(timing.countdownLabelAt(plan, 0.5), "3");
    assert.equal(timing.countdownLabelAt(plan, 1.2), "2");
    assert.equal(timing.countdownLabelAt(plan, 1.6), "1");
    assert.equal(timing.countdownLabelAt(plan, 2.1), "go");
    assert.equal(timing.countdownLabelAt(plan, 2.6), null);
    // Slow steps make the countdown longer than the lead-in.
    assert.ok(Math.abs(timing.countdownPlan(80).total - 2.25) < 1e-9);
  });

  test("pre-roll clock reaches the start point on VIA", () => {
    const plan = timing.countdownPlan(120);
    assert.equal(timing.preRollSongTime(plan, 0, 30), 28);
    assert.equal(timing.preRollSongTime(plan, 1, 30), 29);
    assert.equal(timing.preRollSongTime(plan, 2, 30), 30);
  });
});

describe("note speed as lead time", () => {
  test("1.0x = 1.6 s whatever the stage height", () => {
    assert.equal(timing.leadTimeFor(1), BASE_LEAD_TIME);
    for (const hitY of [400, 700, 1200]) {
      const speed = timing.pxPerSecond(hitY, timing.leadTimeFor(1));
      // A note spawned at the top reaches the hit line after exactly the lead time.
      assert.ok(Math.abs(timing.noteY(10 + BASE_LEAD_TIME, 10, hitY, speed)) < 1e-9);
      assert.equal(timing.noteY(10, 10, hitY, speed), hitY);
    }
  });

  test("multiplier is clamped 0.8x-1.6x", () => {
    assert.equal(timing.leadTimeFor(2), BASE_LEAD_TIME / 1.6);
    assert.equal(timing.leadTimeFor(0.1), BASE_LEAD_TIME / 0.8);
    assert.equal(timing.clampSpeedMultiplier(1.234), 1.25);
  });
});

describe("latency", () => {
  test("game clock lags the audio by the calibration", () => {
    assert.ok(Math.abs(timing.gameTimeFromAudio(10, 80) - 9.92) < 1e-9);
    assert.ok(Math.abs(timing.gameTimeFromAudio(10, 999) - 9.85) < 1e-9);
  });
  test("tap test: median, first taps dropped, needs enough taps", () => {
    assert.equal(timing.estimateLatencyMs([300, -200, 40, 50, 60, 1000, 45, 55]), 50);
    assert.equal(timing.estimateLatencyMs([10, 20, 30]), null);
  });
  test("daily index is stable and in range", () => {
    assert.equal(timing.dailyIndex("2026-10-05", 50), timing.dailyIndex("2026-10-05", 50));
    assert.ok(timing.dailyIndex("2026-10-05", 50) < 50);
    assert.equal(timing.dailyIndex("x", 0), -1);
  });
});
