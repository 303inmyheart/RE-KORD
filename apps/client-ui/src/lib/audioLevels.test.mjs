/**
 * Livelli del player (crossfade, dissolvenza del timer) e conteggio di chi usa
 * l'analizzatore. Si lancia con `pnpm test` (node --experimental-strip-types).
 */
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  clampLevel,
  constantLevel,
  createLeaseCounter,
  createLeaseSwitch,
  elementVolume,
  levelAt,
  rampActive,
  retargetLevel,
} from "./audioLevels.ts";

test("clampLevel tiene tra 0 e 1 e scarta i NaN", () => {
  assert.equal(clampLevel(-1), 0);
  assert.equal(clampLevel(2), 1);
  assert.equal(clampLevel(0.4), 0.4);
  assert.equal(clampLevel(Number.NaN), 0);
});

test("un livello costante non si muove", () => {
  const r = constantLevel(0.7);
  assert.equal(levelAt(r, 0), 0.7);
  assert.equal(levelAt(r, 1e9), 0.7);
  assert.equal(rampActive(r, 0), false);
});

test("la rampa e' lineare e si ferma sul bersaglio", () => {
  const r = retargetLevel(constantLevel(1), 0, 1000, 3000);
  assert.equal(levelAt(r, 1000), 1);
  assert.equal(levelAt(r, 2500), 0.5);
  assert.equal(levelAt(r, 4000), 0);
  assert.equal(levelAt(r, 9000), 0);
  assert.equal(levelAt(r, 500), 1, "prima dell'inizio vale il punto di partenza");
  assert.equal(rampActive(r, 2000), true);
  assert.equal(rampActive(r, 4000), false);
});

test("ripianificare a meta' riparte dal livello attuale, senza salti", () => {
  const fadeOut = retargetLevel(constantLevel(1), 0, 0, 1000);
  const back = retargetLevel(fadeOut, 1, 250, 1000);
  assert.equal(back.from, 0.75);
  assert.equal(levelAt(back, 250), 0.75);
  assert.equal(levelAt(back, 750), 0.875);
  assert.equal(levelAt(back, 1250), 1);
});

test("durata nulla o negativa salta subito al bersaglio", () => {
  const fade = retargetLevel(constantLevel(1), 0, 0, 1000);
  assert.deepEqual(retargetLevel(fade, 1, 500, 0), constantLevel(1));
  assert.deepEqual(retargetLevel(fade, 0.2, 500, -5), constantLevel(0.2));
});

test("bersaglio gia' raggiunto: nessuna rampa da campionare", () => {
  const r = retargetLevel(constantLevel(0), 0, 100, 3000);
  assert.equal(rampActive(r, 200), false);
});

test("il volume dell'elemento e' livello del deck per livello master", () => {
  assert.equal(elementVolume(1, 1), 1);
  assert.equal(elementVolume(0.5, 0.5), 0.25);
  assert.equal(elementVolume(0, 1), 0);
  assert.equal(elementVolume(1, 0), 0);
  assert.equal(elementVolume(0.99995, 1), 1, "le briciole finiscono a pieno volume");
  assert.equal(elementVolume(0.00005, 1), 0, "le briciole finiscono in silenzio");
  assert.equal(elementVolume(2, -1), 0);
});

test("il contatore chiama onFirst e onLast solo ai bordi", () => {
  const calls = [];
  const c = createLeaseCounter(
    () => calls.push("first"),
    () => calls.push("last"),
  );
  const a = c.acquire();
  const b = c.acquire();
  assert.equal(c.count, 2);
  assert.deepEqual(calls, ["first"]);
  a();
  assert.deepEqual(calls, ["first"]);
  b();
  assert.equal(c.count, 0);
  assert.deepEqual(calls, ["first", "last"]);
  const again = c.acquire();
  assert.deepEqual(calls, ["first", "last", "first"]);
  again();
  assert.deepEqual(calls, ["first", "last", "first", "last"]);
});

test("rilasciare due volte la stessa licenza conta una volta", () => {
  let last = 0;
  const c = createLeaseCounter(() => {}, () => (last += 1));
  const a = c.acquire();
  const b = c.acquire();
  a();
  a();
  assert.equal(c.count, 1);
  assert.equal(last, 0);
  b();
  assert.equal(c.count, 0);
  assert.equal(last, 1);
});

test("l'interruttore prende e lascia una sola licenza", () => {
  let first = 0;
  let last = 0;
  const c = createLeaseCounter(() => (first += 1), () => (last += 1));
  const s = createLeaseSwitch(() => c.acquire());
  s.set(true);
  s.set(true);
  assert.equal(c.count, 1);
  assert.equal(s.held, true);
  s.set(false);
  s.set(false);
  assert.equal(c.count, 0);
  assert.equal(s.held, false);
  s.set(true);
  s.dispose();
  s.dispose();
  assert.equal(c.count, 0);
  assert.equal(first, 2);
  assert.equal(last, 2);
});

test("due interruttori: il grafo resta finche' l'ultimo non lascia", () => {
  const events = [];
  const c = createLeaseCounter(() => events.push("engage"), () => events.push("idle"));
  const viz = createLeaseSwitch(() => c.acquire());
  const plectr = createLeaseSwitch(() => c.acquire());
  viz.set(true);
  plectr.set(true);
  viz.dispose();
  assert.deepEqual(events, ["engage"]);
  plectr.set(false);
  assert.deepEqual(events, ["engage", "idle"]);
});

test("quantizeVolume: coarse grid, exact ends, clamped", async () => {
  const { quantizeVolume } = await import("./audioLevels.ts");
  assert.equal(quantizeVolume(0, 0.05), 0);
  assert.equal(quantizeVolume(1, 0.05), 1);
  assert.equal(quantizeVolume(-0.2, 0.05), 0);
  assert.equal(quantizeVolume(1.4, 0.05), 1);
  assert.equal(quantizeVolume(0.512, 0.05), 0.5);
  assert.equal(quantizeVolume(0.526, 0.05), 0.55);
  assert.equal(quantizeVolume(0.013, 0.05), 0);
  assert.equal(quantizeVolume(0.37, 0), 0.37);
  assert.equal(quantizeVolume(Number.NaN, 0.05), 0);
});
