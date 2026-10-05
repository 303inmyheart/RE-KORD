/**
 * Testo del conto alla rovescia del timer di spegnimento.
 * Si lancia con `pnpm test` (node --experimental-strip-types).
 */
import assert from "node:assert/strict";
import { test } from "node:test";
import { formatSleepRemaining } from "./sleepTimerFormat.ts";

test("minuti e secondi sotto l'ora", () => {
  assert.equal(formatSleepRemaining(0), "0:00");
  assert.equal(formatSleepRemaining(65_000), "1:05");
  assert.equal(formatSleepRemaining(59 * 60_000 + 59_999), "59:59");
});

test("ore dall'ora in su", () => {
  assert.equal(formatSleepRemaining(3_600_000), "1:00:00");
  assert.equal(formatSleepRemaining(2 * 3_600_000 + 5 * 60_000 + 7_000), "2:05:07");
});

test("valori strani non rompono", () => {
  assert.equal(formatSleepRemaining(-5), "0:00");
  assert.equal(formatSleepRemaining(Number.NaN), "0:00");
});
