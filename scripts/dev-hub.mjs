#!/usr/bin/env node
// Development without the Tauri shell: hub (cargo run), client UI (vite :7422)
// and admin panel (vite :7421) together, output prefixed, Ctrl+C stops all.
//
//   pnpm dev:hub                       data dir: see below
//   pnpm dev:hub -- --data-dir <dir>   any extra arguments go to rekord-server
//
// Data dir, unless given: REKORD_DATA_DIR, else on Linux the one of the
// RE-KORD Server desktop app (~/.local/share/app.rekord.server/hub) when it
// exists, else rekord-server's own default.

import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { createConnection } from "node:net";
import { homedir, platform } from "node:os";
import { join } from "node:path";

const root = new URL("..", import.meta.url).pathname;
const extra = process.argv.slice(2).filter((a) => a !== "--");

function dataDirArgs() {
  if (extra.includes("--data-dir") || process.env.REKORD_DATA_DIR) return [];
  if (platform() === "linux") {
    const appDir = join(homedir(), ".local/share/app.rekord.server/hub");
    if (existsSync(appDir)) return ["--data-dir", appDir];
  }
  return [];
}

function portInUse(port) {
  return new Promise((resolve) => {
    const sock = createConnection({ port, host: "127.0.0.1" });
    sock.once("connect", () => {
      sock.destroy();
      resolve(true);
    });
    sock.once("error", () => resolve(false));
  });
}

const COLORS = { hub: 33, client: 36, admin: 35 };
const children = [];
let stopping = false;

function run(name, cmd, args, cwd = root) {
  const child = spawn(cmd, args, {
    cwd,
    env: { ...process.env, FORCE_COLOR: "1" },
    stdio: ["ignore", "pipe", "pipe"],
    // Own process group: the terminal's Ctrl+C reaches only this script,
    // which then stops the children in order.
    detached: platform() !== "win32",
  });
  const tag = `\x1b[${COLORS[name]}m[${name}]\x1b[0m `;
  for (const stream of [child.stdout, child.stderr]) {
    let buf = "";
    stream.on("data", (chunk) => {
      buf += chunk;
      const lines = buf.split("\n");
      buf = lines.pop();
      for (const line of lines) process.stdout.write(tag + line + "\n");
    });
  }
  child.on("exit", (code, signal) => {
    process.stdout.write(`${tag}exited (${signal ?? code})\n`);
    if (!stopping) stop(code ?? 1);
  });
  children.push(child);
}

function stop(code = 0) {
  if (stopping) return;
  stopping = true;
  for (const child of children) {
    if (child.exitCode !== null) continue;
    // `cargo run` execs rekord-server and vite stops its own esbuild helper,
    // so the direct child is enough (a group kill made vite log EPIPE noise).
    child.kill("SIGTERM");
  }
  setTimeout(() => process.exit(code), 1500).unref();
}

process.on("SIGINT", () => stop(0));
process.on("SIGTERM", () => stop(0));

for (const port of [7420, 7421, 7422]) {
  if (await portInUse(port)) {
    console.error(
      `Port ${port} is already in use. Close the RE-KORD app or the other dev server first.`,
    );
    process.exit(1);
  }
}

const hubArgs = ["run", "-p", "rekord-server", "--", ...dataDirArgs(), ...extra];
const dataIdx = hubArgs.indexOf("--data-dir");
console.log(
  `RE-KORD dev hub — data: ${dataIdx >= 0 ? hubArgs[dataIdx + 1] : process.env.REKORD_DATA_DIR ?? "rekord-server default"}`,
);
console.log("  app:   http://localhost:7422/");
console.log("  admin: http://localhost:7421/admin/\n");

run("hub", "cargo", hubArgs);
// vite directly (not through pnpm) so Ctrl+C doesn't print pnpm's SIGTERM error.
const vite = (app) => [join(root, "apps", app, "node_modules/.bin/vite"), [], join(root, "apps", app)];
run("client", ...vite("client-ui"));
run("admin", ...vite("server-ui"));
