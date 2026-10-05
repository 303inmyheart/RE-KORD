/**
 * Confronto di versioni client/hub (`platform/semver.ts`) e decisione del banner
 * di aggiornamento (`platform/compat.ts`).
 * Importa i moduli veri: si lancia con `pnpm test` (node --experimental-strip-types).
 */
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  compareSemver,
  diffLevel,
  parseSemver,
  satisfiesMin,
} from "./platform/semver.ts";
import { evaluateCompat, SUPPORTED_API_VERSION } from "./platform/compat.ts";

test("parse: forme tollerate", () => {
  assert.deepEqual(parseSemver("5.1.0"), { major: 5, minor: 1, patch: 0, pre: [] });
  assert.deepEqual(parseSemver("v5.2"), { major: 5, minor: 2, patch: 0, pre: [] });
  assert.deepEqual(parseSemver(" 6 "), { major: 6, minor: 0, patch: 0, pre: [] });
  assert.deepEqual(parseSemver("5.1.0-rc.1+abc"), {
    major: 5,
    minor: 1,
    patch: 0,
    pre: ["rc", "1"],
  });
});

test("parse: spazzatura torna null", () => {
  assert.equal(parseSemver(""), null);
  assert.equal(parseSemver("latest"), null);
  assert.equal(parseSemver("5.1.0.4"), null);
  assert.equal(parseSemver(undefined), null);
  assert.equal(parseSemver(510), null);
});

test("compare: numerico, non alfabetico", () => {
  assert.equal(compareSemver("5.10.0", "5.9.9"), 1);
  assert.equal(compareSemver("5.1.0", "5.1.0"), 0);
  assert.equal(compareSemver("5.1.0", "5.1"), 0);
  assert.equal(compareSemver("4.99.99", "5.0.0"), -1);
  assert.equal(compareSemver("x", "5.0.0"), null);
});

test("compare: pre-release prima della release", () => {
  assert.equal(compareSemver("5.1.0-rc.1", "5.1.0"), -1);
  assert.equal(compareSemver("5.1.0-rc.2", "5.1.0-rc.10"), -1);
  assert.equal(compareSemver("5.1.0-alpha", "5.1.0-1"), 1);
  assert.equal(compareSemver("5.1.0-rc", "5.1.0-rc.1"), -1);
});

test("satisfiesMin: senza minimo leggibile non blocca", () => {
  assert.equal(satisfiesMin("5.1.0", "5.1.0"), true);
  assert.equal(satisfiesMin("5.0.9", "5.1.0"), false);
  assert.equal(satisfiesMin("5.1.0", undefined), true);
  assert.equal(satisfiesMin("garbage", "5.1.0"), true);
});

test("diffLevel", () => {
  assert.equal(diffLevel("5.1.0", "6.0.0"), "major");
  assert.equal(diffLevel("5.1.0", "5.2.0"), "minor");
  assert.equal(diffLevel("5.1.0", "5.1.3"), "patch");
  assert.equal(diffLevel("5.1.0", "5.1.0"), null);
});

const base = { clientVersion: "5.1.0", bundled: true };

test("compat: client sotto il minimo dell'hub → bloccante", () => {
  const r = evaluateCompat({ ...base, health: { version: "5.3.0", minClientVersion: "5.2.0", apiVersion: 1 } });
  assert.equal(r.kind, "client-too-old");
});

test("compat: api dell'hub piu' nuova di quella che il client conosce → bloccante", () => {
  const r = evaluateCompat({
    ...base,
    health: { version: "6.0.0", apiVersion: SUPPORTED_API_VERSION + 1 },
  });
  assert.equal(r.kind, "client-too-old");
});

test("compat: hub piu' nuovo di una minor → avviso morbido", () => {
  const r = evaluateCompat({ ...base, health: { version: "5.2.0", minClientVersion: "5.1.0", apiVersion: 1 } });
  assert.equal(r.kind, "hub-newer");
});

test("compat: una patch dell'hub non disturba", () => {
  const r = evaluateCompat({ ...base, health: { version: "5.1.4", minClientVersion: "5.1.0", apiVersion: 1 } });
  assert.equal(r.kind, "ok");
});

test("compat: hub vecchio senza campi nuovi → nessun avviso", () => {
  const r = evaluateCompat({ ...base, health: { version: "5.1.0" } });
  assert.equal(r.kind, "ok");
});

test("compat: hub piu' vecchio di una minor → avviso morbido per l'hub", () => {
  const r = evaluateCompat({ ...base, clientVersion: "5.3.0", health: { version: "5.1.0", apiVersion: 1 } });
  assert.equal(r.kind, "hub-older");
});

test("compat: pagina servita dall'hub (non impacchettata) piu' vecchia → ricarica", () => {
  const r = evaluateCompat({ clientVersion: "5.1.0", bundled: false, health: { version: "5.2.0", apiVersion: 1 } });
  assert.equal(r.kind, "reload");
});
