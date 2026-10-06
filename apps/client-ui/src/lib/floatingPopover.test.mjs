/**
 * Placement of floating menus (album "Add genre", track-row ⋯ / playlist menus):
 * below the anchor by default, flipped above when there is more room there,
 * clamped inside the viewport, height capped to the available room.
 */
import assert from "node:assert/strict";
import { test } from "node:test";

const { placeFloating } = await import("./floatingPopover.ts");

const VP = { width: 1000, height: 800 };
const anchor = (top, left = 100, width = 120, height = 30) => ({
  top,
  bottom: top + height,
  left,
  right: left + width,
  width,
});

test("opens below the anchor when it fits", () => {
  const p = placeFloating(anchor(100), { width: 200, height: 300 }, VP);
  assert.equal(p.side, "bottom");
  assert.equal(p.top, 134);
  assert.equal(p.left, 100);
  assert.ok(p.maxHeight >= 300);
});

test("flips above near the bottom of the viewport", () => {
  const p = placeFloating(anchor(700), { width: 200, height: 300 }, VP);
  assert.equal(p.side, "top");
  assert.equal(p.top, 700 - 4 - 300);
});

test("end alignment and viewport clamping", () => {
  const end = placeFloating(anchor(100, 800, 100), { width: 250, height: 100 }, VP, "bottom-end");
  assert.equal(end.left, 900 - 250);
  const clamped = placeFloating(anchor(100, 900, 80), { width: 250, height: 100 }, VP);
  assert.equal(clamped.left, 1000 - 8 - 250);
});

test("a tall list is capped to the room it has and scrolls", () => {
  const p = placeFloating(anchor(380), { width: 200, height: 2000 }, VP);
  assert.ok(p.maxHeight < 800);
  assert.ok(p.top >= 0);
});
