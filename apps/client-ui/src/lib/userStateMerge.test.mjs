/**
 * Merge a tre vie dello stato utente dopo un 409 (due dispositivi insieme).
 * Si lancia con `pnpm test` (node --experimental-strip-types).
 */
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  agreedBase,
  changedUserFields,
  conflictState,
  pushRetryDelay,
  rebaseMoods,
  rebasePlayCounts,
  rebaseRecent,
  rebaseSet,
  rebaseUserState,
  sameJsonValue,
  syncedFieldsOf,
} from "./userStateMerge.ts";

const empty = () => ({
  playCounts: {},
  recentRelPaths: [],
  trackMoods: {},
  excludedRelPaths: [],
  excludedAlbumIds: [],
});

test("ascolti additivi: i due dispositivi sommano", () => {
  const base = { a: 2, b: 1 };
  const local = { a: 4, b: 1, c: 1 };
  const server = { a: 3, b: 5 };
  assert.deepEqual(rebasePlayCounts(base, local, server), { a: 5, b: 5, c: 1 });
});

test("ascolti: chiave tolta qui sparisce se il server non ha contato altro", () => {
  assert.deepEqual(rebasePlayCounts({ "12": 3 }, { "x.mp3": 3 }, { "12": 3 }), {
    "x.mp3": 3,
  });
  // Altrove qualcuno ha ascoltato ancora: si tiene il numero del server.
  assert.deepEqual(rebasePlayCounts({ "12": 3 }, {}, { "12": 4 }), { "12": 4 });
});

test("recenti: i nuovi ascolti locali in testa, poi quelli del server", () => {
  const base = ["a", "b", "c"];
  const local = ["x", "a", "b", "c"];
  const server = ["y", "a", "b", "c"];
  assert.deepEqual(rebaseRecent(base, local, server), ["x", "y", "a", "b", "c"]);
  // Niente di nuovo qui: vince il server.
  assert.deepEqual(rebaseRecent(base, base, server), server);
  // Riascolto di un brano gia' presente: torna in testa.
  assert.deepEqual(rebaseRecent(base, ["c", "a", "b"], server), ["c", "y", "a", "b"]);
});

test("recenti: tetto rispettato", () => {
  const out = rebaseRecent([], ["a", "b"], ["c", "d"], 3);
  assert.deepEqual(out, ["a", "b", "c"]);
});

test("mood per chiave: vince chi ha cambiato quella chiave", () => {
  const base = { a: ["calm"], b: ["dark"] };
  const local = { a: ["happy"], b: ["dark"] };
  const server = { a: ["calm"], b: ["epic"], c: ["new"] };
  assert.deepEqual(rebaseMoods(base, local, server), {
    a: ["happy"],
    b: ["epic"],
    c: ["new"],
  });
  // Rimozione locale propagata.
  assert.deepEqual(rebaseMoods({ a: ["x"] }, {}, { a: ["x"], b: ["y"] }), { b: ["y"] });
});

test("esclusioni come insiemi", () => {
  assert.deepEqual(rebaseSet(["a", "b"], ["b", "c"], ["a", "b", "d"]).sort(), ["b", "c", "d"]);
  assert.deepEqual(rebaseSet([1], [1, 2], [3]).sort(), [2, 3]);
});

test("changedUserFields: solo i campi toccati", () => {
  const base = empty();
  const local = { ...empty(), playCounts: { a: 1 } };
  assert.deepEqual(changedUserFields(base, local), ["playCounts"]);
  assert.equal(changedUserFields(null, local).length, 5);
  assert.deepEqual(changedUserFields(base, empty()), []);
});

test("rebaseUserState mette insieme tutto", () => {
  const base = { ...empty(), playCounts: { a: 1 }, excludedAlbumIds: [1] };
  const local = { ...empty(), playCounts: { a: 2 }, excludedAlbumIds: [] };
  const server = { ...empty(), playCounts: { a: 3 }, excludedAlbumIds: [1, 7] };
  const out = rebaseUserState(base, local, server);
  assert.deepEqual(out.playCounts, { a: 4 });
  assert.deepEqual(out.excludedAlbumIds, [7]);
});

test("syncedFieldsOf ripulisce un payload incompleto", () => {
  const out = syncedFieldsOf({ playCounts: { a: "3", b: "x" }, trackMoods: { t: ["m", 1] } });
  assert.deepEqual(out.playCounts, { a: 3 });
  assert.deepEqual(out.trackMoods, { t: ["m"] });
  assert.deepEqual(out.recentRelPaths, []);
  assert.deepEqual(syncedFieldsOf(null), empty());
});

test("conflictState legge il corpo del 409", () => {
  const cur = { revision: 9, playCounts: {} };
  assert.equal(conflictState({ error: "revision_conflict", current: cur }), cur);
  assert.equal(conflictState({ ok: false, error: "revision conflict: have 9" }), null);
  assert.equal(conflictState(null), null);
});

test("pushRetryDelay cresce e si ferma a un minuto", () => {
  assert.equal(pushRetryDelay(0), 2000);
  assert.equal(pushRetryDelay(1), 4000);
  assert.equal(pushRetryDelay(20), 60_000);
});

test("sameJsonValue: l'ordine delle chiavi (mappa ordinata dell'hub) non conta", () => {
  // Come esce da mergePlectrStores e come torna dall'hub (chiavi ordinate).
  const mine = { version: 1, difficulty: "normal", bests: { b: { s: 2 }, a: { s: 1 } }, lowEnd: null };
  const hub = { bests: { a: { s: 1 }, b: { s: 2 } }, difficulty: "normal", lowEnd: null, version: 1 };
  assert.notEqual(JSON.stringify(mine), JSON.stringify(hub));
  assert.equal(sameJsonValue(mine, hub), true);
  assert.equal(sameJsonValue({ ...hub, version: 2 }, mine), false);
  assert.equal(sameJsonValue({ a: [1, 2] }, { a: [2, 1] }), false);
  assert.equal(sameJsonValue({ a: 1, u: undefined }, { a: 1 }), true);
  assert.equal(sameJsonValue({ a: null }, { a: undefined }), false);
  assert.equal(sameJsonValue(undefined, null), false);
  assert.equal(sameJsonValue([], {}), false);
});

test("push arrivato ma risposta persa (timeout, pagina chiusa): niente ascolti contati due volte", () => {
  const base = { ...empty(), playCounts: { a: 2 } };
  const sent = { ...empty(), playCounts: { a: 3 } }; // giornale prima della PATCH
  const server = { ...empty(), playCounts: { a: 3 }, trackMoods: {} }; // la PATCH e' passata
  // Senza giornale si ribasava sulla base vecchia: 3 + (3 - 2) = 4.
  assert.deepEqual(rebaseUserState(base, sent, server).playCounts, { a: 4 });
  const agreed = agreedBase(base, sent, server);
  assert.equal(agreed, sent);
  assert.deepEqual(changedUserFields(agreed, sent), []);
  // Un ascolto in piu' fatto dopo: si somma una volta sola.
  const local = { ...empty(), playCounts: { a: 4 } };
  assert.deepEqual(rebaseUserState(agreed, local, server).playCounts, { a: 4 });
});

test("agreedBase: PATCH non arrivata o hub cambiato da altri, resta la base", () => {
  const base = { ...empty(), playCounts: { a: 2 } };
  const sent = { ...empty(), playCounts: { a: 3 } };
  assert.equal(agreedBase(base, sent, base), base);
  const other = { ...empty(), playCounts: { a: 3, b: 1 } };
  assert.equal(agreedBase(base, sent, other), base);
  assert.equal(agreedBase(base, null, sent), base);
  // Mood con chiavi riordinate dall'hub contano come uguali.
  const sentMoods = { ...sent, trackMoods: { z: ["calm"], a: ["dark"] } };
  const hubMoods = { ...sent, trackMoods: { a: ["dark"], z: ["calm"] } };
  assert.equal(agreedBase(base, sentMoods, hubMoods), sentMoods);
});
