import { readFileSync } from "node:fs";
import type { ServerResponse } from "node:http";
import { defineConfig, type Plugin, type ProxyOptions } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

const host = process.env.TAURI_DEV_HOST;

/** When the hub is down, default http-proxy sends 500 with an empty body → client `.json()` throws. */
function hubProxy(target: string): ProxyOptions {
  return {
    target,
    configure(proxy) {
      proxy.on("error", (_err, _req, res) => {
        const r = res as ServerResponse | undefined;
        if (!r || r.headersSent || typeof r.writeHead !== "function") return;
        r.writeHead(502, { "Content-Type": "application/json; charset=utf-8" });
        r.end(JSON.stringify({ ok: false, error: "Hub non raggiungibile" }));
      });
    },
  };
}

/** Sources of optional modules, whose chunks are never precached. */
const OPTIONAL_MODULE_RE = /[\\/](podcasts)[\\/]|PodcastsView\.svelte|podcastModel\.ts/;

/** Above this size a file does not go into the service worker's initial cache. */
const SW_PRECACHE_MAX_BYTES = 2 * 1024 * 1024;

/**
 * PWA service worker (only the client served by the hub, never in the Tauri shells:
 * see src/lib/platform/pwa.ts). It is generated at the end of the build because it must know the
 * hashed names of the produced files: the template lives in
 * src/lib/platform/serviceWorker.template.js.
 */
function rekordServiceWorker(): Plugin {
  return {
    name: "rekord-service-worker",
    apply: "build",
    generateBundle(_options, bundle) {
      // Optional modules (Podcast e notizie) stay out of the install-time
      // cache: a hub with the module off never downloads its code.
      const moduleChunkNames = new Set(
        Object.values(bundle)
          .filter(
            (item) =>
              item.type === "chunk" &&
              Object.keys(item.modules).length > 0 &&
              Object.keys(item.modules).every((id) => OPTIONAL_MODULE_RE.test(id)),
          )
          .map((item) => item.fileName.replace(/-[\w-]+\.js$/, "")),
      );
      const isModuleFile = (fileName: string) =>
        [...moduleChunkNames].some((base) => fileName.startsWith(`${base}-`));
      const files = Object.values(bundle)
        .filter((item) => !item.fileName.endsWith(".map"))
        .filter((item) => !isModuleFile(item.fileName))
        .filter((item) => {
          const size =
            item.type === "chunk"
              ? item.code.length
              : typeof item.source === "string"
                ? item.source.length
                : item.source.byteLength;
          return size <= SW_PRECACHE_MAX_BYTES;
        })
        .map((item) => `/${item.fileName}`)
        .filter((path) => path !== "/index.html");
      // Files in public/ don't go through the bundle: those the shell needs
      // (icons, logo) are listed by hand.
      const publicFiles = [
        "/manifest.webmanifest",
        "/favicon.ico",
        "/REKORDlogo.png",
        "/icons/icon-192.png",
        "/icons/icon-512.png",
      ];
      const precache = [...new Set(["/", "/index.html", ...publicFiles, ...files])].sort();
      const version = `${process.env.npm_package_version ?? "0"}-${Date.now().toString(36)}`;
      const template = readFileSync(
        new URL("./src/lib/platform/serviceWorker.template.js", import.meta.url),
        "utf8",
      );
      this.emitFile({
        type: "asset",
        fileName: "sw.js",
        source: template
          .replace("__REKORD_SW_VERSION__", version)
          .replace("__REKORD_SW_PRECACHE__", JSON.stringify(precache)),
      });
    },
  };
}

export default defineConfig({
  plugins: [svelte(), rekordServiceWorker()],
  clearScreen: false,
  server: {
    port: 7422,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 7422 } : undefined,
    proxy: {
      "/api": hubProxy("http://127.0.0.1:7420"),
      "/media": hubProxy("http://127.0.0.1:7420"),
    },
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari14",
  },
});
