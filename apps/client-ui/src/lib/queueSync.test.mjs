/** Coda dell'account sull'hub: lista + cursore, il cursore piu' recente vince se ancora valido. */
import assert from "node:assert/strict";
import { test } from "node:test";
import { parseHubQueue } from "./queueSync.ts";

const q = { relPaths: ["a", "b", "c"], currentIndex: 0, relPath: "a", time: 5, updatedAt: 100 };

test("niente coda: null", () => {
  assert.equal(parseHubQueue(null, null), null);
  assert.equal(parseHubQueue({ relPaths: [] }, null), null);
});

test("il cursore piu' recente sposta indice e posizione", () => {
  const s = parseHubQueue(q, { currentIndex: 2, relPath: "c", time: 42, updatedAt: 200 });
  assert.deepEqual([s.index, s.relPath, s.time, s.updatedAt], [2, "c", 42, 200]);
});

test("un cursore vecchio o che non punta piu' alla lista e' ignorato", () => {
  assert.equal(parseHubQueue(q, { currentIndex: 2, relPath: "c", time: 1, updatedAt: 50 }).index, 0);
  assert.equal(parseHubQueue(q, { currentIndex: 2, relPath: "zz", time: 1, updatedAt: 500 }).index, 0);
});

test("indice fuori lista: si ferma all'ultimo brano, senza posizione", () => {
  const s = parseHubQueue({ relPaths: ["a", "b"], currentIndex: 9, relPath: "x", time: 30 }, null);
  assert.deepEqual([s.index, s.relPath, s.time], [1, "b", 0]);
});
