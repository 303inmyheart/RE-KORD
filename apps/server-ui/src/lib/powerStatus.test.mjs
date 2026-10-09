import assert from "node:assert/strict";
import { describe, test } from "node:test";
import { powerActivity, powerLidLine, powerStatusLine } from "./powerStatus.ts";

/** Knows every key: `errors.code.*` come back as `T:<key>`, the others with their vars. */
const tr = (key, vars) =>
  vars ? `${key}${JSON.stringify(vars)}` : key.startsWith("errors.code.") ? `T:${key}` : key;
const fmt = (iso) => iso.slice(11, 16);

function state(over = {}, status = {}) {
  return {
    preventSleep: "whenActive",
    graceMinutes: 10,
    keepAwakeLidClosed: false,
    lockedByEnv: false,
    platform: "linux",
    limits: { minGraceMinutes: 1, maxGraceMinutes: 120, defaultGraceMinutes: 10 },
    ...over,
    status: {
      inhibiting: false,
      reason: null,
      since: null,
      lastActivity: null,
      lastActivityKind: null,
      activeStreams: 0,
      activeJobs: 0,
      releaseAt: null,
      method: "systemd-inhibit",
      supported: true,
      lidSupported: true,
      lidInhibited: false,
      errorCode: null,
      error: null,
      lidErrorCode: null,
      lidError: null,
      ...status,
    },
  };
}

describe("power status line", () => {
  test("off and idle", () => {
    assert.deepEqual(powerStatusLine(state({ preventSleep: "off" }), tr, fmt), {
      tone: "off",
      text: "power.status.off",
    });
    const idle = powerStatusLine(
      state({}, { lastActivity: "2026-10-08T12:03:00Z", lastActivityKind: "stream" }),
      tr,
      fmt,
    );
    assert.equal(idle.tone, "off");
    assert.equal(idle.text, 'power.status.idle power.status.last{"time":"12:03"}');
  });

  test("blocked always, or while in use with the reason", () => {
    const always = powerStatusLine(
      state({ preventSleep: "always" }, { inhibiting: true, reason: "always", since: "2026-10-08T12:03:00Z" }),
      tr,
      fmt,
    );
    assert.deepEqual(always, { tone: "on", text: 'power.status.always{"time":"12:03"}' });

    const busy = state({}, { inhibiting: true, reason: "active", since: "2026-10-08T12:03:00Z", activeStreams: 1 });
    assert.equal(powerActivity(busy), "stream");
    assert.equal(
      powerStatusLine(busy, tr, fmt).text,
      'power.status.active{"what":"power.what.stream","time":"12:03"}',
    );

    const grace = state(
      {},
      {
        inhibiting: true,
        reason: "active",
        since: "2026-10-08T12:03:00Z",
        lastActivityKind: "remote",
        releaseAt: "2026-10-08T12:20:00Z",
      },
    );
    assert.equal(powerActivity(grace), "remote");
    assert.match(powerStatusLine(grace, tr, fmt).text, /power\.status\.until\{"time":"12:20"\}$/);
    assert.equal(powerActivity(state({}, { activeJobs: 2 })), "job");
    assert.equal(powerActivity(state()), "recent");
  });

  test("platform refusals are errors, unknown codes fall back to the detail", () => {
    const refused = powerStatusLine(
      state({ preventSleep: "always" }, { errorCode: "power_refused", error: "Access denied" }),
      tr,
      fmt,
    );
    assert.equal(refused.tone, "error");
    assert.equal(refused.text, 'power.status.error{"reason":"T:errors.code.power_refused"}');

    // A translator without the key returns it unchanged.
    const odd = powerStatusLine(
      state({ preventSleep: "always" }, { errorCode: "power_x", error: "weird" }),
      (k, v) => (v ? `${k}${JSON.stringify(v)}` : k),
      fmt,
    );
    assert.equal(odd.text, 'power.status.error{"reason":"weird"}');
    // An error left over from a mode that is now off is not shown.
    assert.equal(
      powerStatusLine(state({ preventSleep: "off" }, { errorCode: "power_refused" }), tr, fmt).tone,
      "off",
    );
  });

  test("lid refusal only while the lid option is on", () => {
    const lid = state(
      { keepAwakeLidClosed: true },
      { inhibiting: true, reason: "active", lidErrorCode: "power_refused" },
    );
    assert.equal(powerLidLine(lid, tr), 'power.status.lidError{"reason":"T:errors.code.power_refused"}');
    assert.equal(powerLidLine(state({}, { inhibiting: true, lidErrorCode: "power_refused" }), tr), "");
  });
});
