/**
 * Global shortcut matching: which keydowns AppShell acts on and which it must
 * leave to the focused control / the browser.
 */
import assert from "node:assert/strict";
import { test } from "node:test";
import { shortcutActionFor, shortcutItems } from "./shortcutList.ts";

/** Fake element: `matches` lists the selectors `closest()` should satisfy. */
function el(tagName, { matches = [], type = null, editable = false } = {}) {
  return {
    tagName,
    isContentEditable: editable,
    getAttribute: (n) => (n === "type" ? type : null),
    closest: (sel) =>
      sel
        .split(",")
        .map((s) => s.trim())
        .some((s) => matches.includes(s))
        ? {}
        : null,
  };
}

const body = el("BODY");

function key(k, extra = {}) {
  return {
    key: k,
    code: k === " " ? "Space" : undefined,
    ctrlKey: false,
    metaKey: false,
    altKey: false,
    shiftKey: false,
    target: body,
    ...extra,
  };
}

test("plain keys on the page map to actions", () => {
  assert.equal(shortcutActionFor(key(" ")), "play");
  assert.equal(shortcutActionFor(key("ArrowLeft")), "seekBack");
  assert.equal(shortcutActionFor(key("ArrowRight")), "seekForward");
  assert.equal(shortcutActionFor(key("i")), "listen");
  assert.equal(shortcutActionFor(key("s")), "shuffle");
  assert.equal(shortcutActionFor(key("p")), "plectr");
  assert.equal(shortcutActionFor(key("/")), "search");
  assert.equal(shortcutActionFor(key("k", { ctrlKey: true })), "search");
  assert.equal(shortcutActionFor(key("k", { metaKey: true })), "search");
});

test("modifiers belong to the browser, except Ctrl+K and Shift+/", () => {
  assert.equal(shortcutActionFor(key("s", { ctrlKey: true })), null);
  assert.equal(shortcutActionFor(key("p", { metaKey: true })), null);
  assert.equal(shortcutActionFor(key("ArrowLeft", { altKey: true })), null);
  assert.equal(shortcutActionFor(key(" ", { shiftKey: true })), null);
  assert.equal(shortcutActionFor(key("S", { shiftKey: true })), null);
  assert.equal(shortcutActionFor(key("/", { shiftKey: true })), "search");
});

test("text fields and key-consuming widgets swallow everything", () => {
  const input = el("INPUT", { type: "text" });
  assert.equal(shortcutActionFor(key(" ", { target: input })), null);
  assert.equal(shortcutActionFor(key("k", { ctrlKey: true, target: input })), null);
  const slider = el("DIV", { matches: ["[role='slider']"] });
  assert.equal(shortcutActionFor(key("ArrowRight", { target: slider })), null);
  const inDialog = el("BUTTON", { matches: ["button", "[role='dialog']"] });
  assert.equal(shortcutActionFor(key("s", { target: inDialog })), null);
  const editable = el("DIV", { editable: true });
  assert.equal(shortcutActionFor(key("p", { target: editable })), null);
});

test("focused buttons keep Space and arrows, letters still work", () => {
  const button = el("BUTTON", { matches: ["button"] });
  assert.equal(shortcutActionFor(key(" ", { target: button })), null);
  assert.equal(shortcutActionFor(key("ArrowLeft", { target: button })), null);
  assert.equal(shortcutActionFor(key("s", { target: button })), "shuffle");
});

test("open modal, handled events and auto-repeat toggles are ignored", () => {
  assert.equal(shortcutActionFor(key(" "), { modalOpen: true }), null);
  assert.equal(shortcutActionFor(key("p"), { suspended: true }), null);
  assert.equal(shortcutActionFor(key(" ", { defaultPrevented: true })), null);
  assert.equal(shortcutActionFor(key(" ", { repeat: true })), null);
  assert.equal(shortcutActionFor(key("s", { repeat: true })), null);
  // Holding an arrow keeps seeking.
  assert.equal(shortcutActionFor(key("ArrowRight", { repeat: true })), "seekForward");
});

test("settings list covers every action", () => {
  const ids = shortcutItems((k) => k).map((i) => i.id);
  assert.deepEqual(ids, ["search", "play", "seek", "listen", "shuffle", "plectr"]);
});
