/** PWA service worker: audio streams bypass it (static routes) or pass straight through. */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import vm from "node:vm";

const source = readFileSync(new URL("./platform/serviceWorker.template.js", import.meta.url), "utf8")
  .replace("__REKORD_SW_VERSION__", "test")
  .replace("__REKORD_SW_PRECACHE__", '["/", "/index.html"]');

function load() {
  const listeners = {};
  const fetched = [];
  const self = {
    location: { origin: "http://hub" },
    addEventListener: (type, fn) => (listeners[type] = fn),
    skipWaiting: () => Promise.resolve(),
    clients: { claim: () => Promise.resolve() },
  };
  const ctx = {
    self,
    URL,
    caches: { open: async () => ({ addAll: async () => {}, put: async () => {} }), match: async () => undefined, keys: async () => [] },
    fetch: (req) => {
      fetched.push(req.url);
      return Promise.resolve({ ok: true, clone: () => ({}) });
    },
    Response: { error: () => ({}) },
  };
  vm.runInNewContext(source, ctx);
  return { listeners, fetched };
}

function request(path, { range = false, destination = "", mode = "cors" } = {}) {
  return {
    url: `http://hub${path}`,
    method: "GET",
    mode,
    destination,
    headers: { has: (h) => range && h.toLowerCase() === "range" },
  };
}

function dispatch(sw, req) {
  let responded = false;
  sw.listeners.fetch({ request: req, respondWith: () => (responded = true) });
  return responded;
}

test("install registers network-only static routes for every stream path", () => {
  const sw = load();
  let routes = null;
  sw.listeners.install({ addRoutes: (r) => (routes = r), waitUntil: () => {} });
  // Built in the worker's realm: compare as plain data.
  const paths = JSON.parse(JSON.stringify(routes.filter((r) => r.condition.urlPattern).map((r) => r.condition.urlPattern.pathname)));
  assert.deepEqual(paths, ["/media/*", "/api/v1/media/*", "/api/v1/transcode/*", "/api/v1/podcasts/play/*"]);
  assert.ok(routes.every((r) => r.source === "network"));
});

test("without static routes, streams are answered straight from the network", () => {
  const sw = load();
  for (const req of [
    request("/media/Artist/Album/01.m4a"),
    request("/api/v1/transcode/a.wma?format=flac"),
    request("/api/v1/podcasts/play/3/abc"),
    request("/anything", { range: true }),
    request("/x.mp3", { destination: "audio" }),
  ]) {
    assert.equal(dispatch(sw, req), true, req.url);
  }
  assert.equal(sw.fetched.length, 5);
});

test("other API calls are left to the browser; app files still come from the cache", () => {
  const sw = load();
  assert.equal(dispatch(sw, request("/api/v1/user-state")), false);
  assert.equal(dispatch(sw, request("/assets/index-abc.js")), true);
});
