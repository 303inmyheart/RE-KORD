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

/** Oltre questa dimensione un file non entra nella cache iniziale del service worker. */
const SW_PRECACHE_MAX_BYTES = 2 * 1024 * 1024;

/**
 * Service worker della PWA (solo il client servito dall'hub, mai nei gusci Tauri:
 * vedi src/lib/platform/pwa.ts). Si genera a fine build perche' deve conoscere i
 * nomi con hash dei file prodotti: il template sta in
 * src/lib/platform/serviceWorker.template.js.
 */
function rekordServiceWorker(): Plugin {
  return {
    name: "rekord-service-worker",
    apply: "build",
    generateBundle(_options, bundle) {
      const files = Object.values(bundle)
        .filter((item) => !item.fileName.endsWith(".map"))
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
      // I file di public/ non passano dal bundle: quelli che servono al guscio
      // (icone, logo) si elencano a mano.
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
