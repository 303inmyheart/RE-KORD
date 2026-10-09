/**
 * Desktop width settings (per device): normalisation, CSS variables, storage.
 *
 * Si lancia con `pnpm test` (node --experimental-strip-types).
 */
import assert from "node:assert/strict";
import { beforeEach, describe, test } from "node:test";

const store = new Map();
globalThis.localStorage = {
  getItem: (k) => (store.has(k) ? store.get(k) : null),
  setItem: (k, v) => store.set(k, String(v)),
  removeItem: (k) => store.delete(k),
  clear: () => store.clear(),
};

const {
  applyLayoutWidth,
  clampLayoutWidth,
  CONTENT_MAX_VAR,
  DOCK_MAX_VAR,
  layoutWidthCssVars,
  loadLayoutWidth,
  normalizeLayoutWidth,
  saveLayoutWidth,
} = await import("./layoutWidth.ts");

function fakeRoot() {
  const props = new Map();
  return {
    props,
    style: {
      setProperty: (k, v) => props.set(k, v),
      removeProperty: (k) => props.delete(k),
    },
  };
}

describe("layout width prefs", () => {
  beforeEach(() => store.clear());

  test("defaults when nothing is stored or the blob is broken", () => {
    assert.deepEqual(loadLayoutWidth(), { content: "default", dock: "default" });
    store.set("rekord.next.device.layoutWidth", "{not json");
    assert.deepEqual(loadLayoutWidth(), { content: "default", dock: "default" });
  });

  test("normalises values into range and steps", () => {
    assert.equal(clampLayoutWidth(100), 960);
    assert.equal(clampLayoutWidth(9999), 2560);
    assert.equal(clampLayoutWidth(1447), 1440);
    assert.deepEqual(normalizeLayoutWidth({ content: "1500", dock: "content" }), {
      content: 1500,
      dock: "content",
    });
    assert.deepEqual(normalizeLayoutWidth({ content: "content", dock: -4 }), {
      content: "default",
      dock: "default",
    });
    assert.deepEqual(normalizeLayoutWidth(null), { content: "default", dock: "default" });
  });

  test("CSS variables: default leaves the built-in width", () => {
    assert.deepEqual(layoutWidthCssVars({ content: "default", dock: "default" }), {
      content: null,
      dock: null,
    });
    assert.deepEqual(layoutWidthCssVars({ content: "full", dock: "full" }), {
      content: "none",
      dock: "none",
    });
    assert.deepEqual(layoutWidthCssVars({ content: 1440, dock: 1200 }), {
      content: "1440px",
      dock: "1200px",
    });
  });

  test("player bar can follow the content width", () => {
    assert.equal(layoutWidthCssVars({ content: 1680, dock: "content" }).dock, "1680px");
    assert.equal(layoutWidthCssVars({ content: "full", dock: "content" }).dock, "none");
    assert.equal(layoutWidthCssVars({ content: "default", dock: "content" }).dock, "1360px");
  });

  test("save / load round trip; all-default clears the key", () => {
    saveLayoutWidth({ content: 1440, dock: 1200 });
    assert.deepEqual(loadLayoutWidth(), { content: 1440, dock: 1200 });
    saveLayoutWidth({ content: "default", dock: "default" });
    assert.equal(store.has("rekord.next.device.layoutWidth"), false);
  });

  test("storage failures never throw", () => {
    const saved = globalThis.localStorage;
    globalThis.localStorage = {
      getItem() {
        throw new Error("denied");
      },
      setItem() {
        throw new Error("quota");
      },
      removeItem() {
        throw new Error("denied");
      },
    };
    try {
      assert.deepEqual(loadLayoutWidth(), { content: "default", dock: "default" });
      assert.doesNotThrow(() => saveLayoutWidth({ content: 1200, dock: "full" }));
    } finally {
      globalThis.localStorage = saved;
    }
  });

  test("apply sets and removes the root variables", () => {
    const root = fakeRoot();
    applyLayoutWidth({ content: 1440, dock: "full" }, root);
    assert.equal(root.props.get(CONTENT_MAX_VAR), "1440px");
    assert.equal(root.props.get(DOCK_MAX_VAR), "none");
    applyLayoutWidth({ content: "default", dock: "default" }, root);
    assert.equal(root.props.size, 0);
  });
});
