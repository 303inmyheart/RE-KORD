#!/usr/bin/env node
/**
 * Una sola versione per tutto RE-KORD next.
 *
 *   node scripts/version.mjs sync 5.2.0   scrive 5.2.0 ovunque
 *   node scripts/version.mjs sync         riallinea tutto alla versione di package.json
 *   node scripts/version.mjs check 5.2.0  verifica che ovunque ci sia 5.2.0
 *   node scripts/version.mjs check        verifica che tutto combaci con package.json
 *
 * Dove vive la versione:
 *   - package.json (radice, apps/*, packages/*)
 *   - Cargo.toml `[workspace.package] version` (i crate usano version.workspace)
 *   - Cargo.lock, voci dei crate del workspace (altrimenti `cargo --locked` fallisce)
 *   - apps/client-shell/src-tauri/tauri.conf.json `version`, se presente (di
 *     norma assente: Tauri la prende da Cargo.toml)
 *   - apps/client-ui/src/lib/version.ts `APP_VERSION`
 *
 * `check` esce con codice 1 alla prima discrepanza: la CI lo usa (next.yml).
 * Niente dipendenze: gira con il solo node.
 */
import { readFileSync, writeFileSync, existsSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;

const rel = (p) => path.relative(ROOT, p) || p;
const read = (p) => readFileSync(p, "utf8");

function packageJsonFiles() {
  const out = [path.join(ROOT, "package.json")];
  for (const group of ["apps", "packages"]) {
    const dir = path.join(ROOT, group);
    if (!existsSync(dir)) continue;
    for (const name of readdirSync(dir).sort()) {
      const p = path.join(dir, name, "package.json");
      if (existsSync(p)) out.push(p);
    }
  }
  return out;
}

/** Nomi dei crate del workspace, letti dai loro Cargo.toml. */
function workspaceCrates() {
  const cargo = read(path.join(ROOT, "Cargo.toml"));
  const members = /members\s*=\s*\[([\s\S]*?)\]/.exec(cargo)?.[1] ?? "";
  const names = [];
  for (const m of members.matchAll(/"([^"]+)"/g)) {
    const toml = path.join(ROOT, m[1], "Cargo.toml");
    if (!existsSync(toml)) continue;
    const name = /^\s*name\s*=\s*"([^"]+)"/m.exec(read(toml))?.[1];
    if (name) names.push(name);
  }
  return names;
}

/**
 * Ogni "luogo" sa leggere la propria versione e scriverne una nuova
 * (preservando il resto del file byte per byte).
 */
function locations() {
  const list = [];

  for (const file of packageJsonFiles()) {
    list.push({
      file,
      label: rel(file),
      get: (src) => JSON.parse(src).version ?? null,
      set: (src, v) => src.replace(/("version"\s*:\s*")[^"]*(")/, `$1${v}$2`),
    });
  }

  const cargoToml = path.join(ROOT, "Cargo.toml");
  const wsVersion = /(\[workspace\.package\][^[]*?\bversion\s*=\s*")([^"]*)(")/;
  list.push({
    file: cargoToml,
    label: "Cargo.toml [workspace.package]",
    get: (src) => wsVersion.exec(src)?.[2] ?? null,
    set: (src, v) => src.replace(wsVersion, `$1${v}$3`),
  });

  const lock = path.join(ROOT, "Cargo.lock");
  if (existsSync(lock)) {
    for (const crate of workspaceCrates()) {
      const re = new RegExp(`(\\[\\[package\\]\\]\\nname = "${crate}"\\nversion = ")([^"]*)(")`);
      list.push({
        file: lock,
        label: `Cargo.lock (${crate})`,
        optional: true,
        get: (src) => re.exec(src)?.[2] ?? null,
        set: (src, v) => src.replace(re, `$1${v}$3`),
      });
    }
  }

  const tauriConf = path.join(ROOT, "apps/client-shell/src-tauri/tauri.conf.json");
  if (existsSync(tauriConf)) {
    list.push({
      file: tauriConf,
      label: "tauri.conf.json",
      // Assente = eredita da Cargo.toml: va bene, non c'e' niente da allineare.
      optional: true,
      get: (src) => {
        const v = JSON.parse(src).version;
        return typeof v === "string" && SEMVER.test(v) ? v : null;
      },
      set: (src, v) => src.replace(/("version"\s*:\s*")\d[^"]*(")/, `$1${v}$2`),
    });
  }

  const versionTs = path.join(ROOT, "apps/client-ui/src/lib/version.ts");
  list.push({
    file: versionTs,
    label: "client-ui/src/lib/version.ts",
    get: (src) => /APP_VERSION\s*=\s*"([^"]*)"/.exec(src)?.[1] ?? null,
    set: (src, v) => src.replace(/(APP_VERSION\s*=\s*")[^"]*(")/, `$1${v}$2`),
  });

  return list;
}

function usage(code = 2) {
  console.error("Uso: node scripts/version.mjs <sync|check> [x.y.z]");
  process.exit(code);
}

const [cmd, wanted] = process.argv.slice(2);
if (cmd !== "sync" && cmd !== "check") usage();

const rootVersion = JSON.parse(read(path.join(ROOT, "package.json"))).version;
const target = wanted ?? rootVersion;
if (!SEMVER.test(target)) {
  console.error(`Versione non valida: ${target}`);
  process.exit(2);
}

const locs = locations();
let bad = 0;

if (cmd === "check") {
  for (const loc of locs) {
    if (!existsSync(loc.file)) {
      if (!loc.optional) {
        console.error(`MANCA  ${loc.label}`);
        bad++;
      }
      continue;
    }
    const found = loc.get(read(loc.file));
    if (found == null) {
      if (!loc.optional) {
        console.error(`??     ${loc.label}: versione non trovata`);
        bad++;
      }
      continue;
    }
    if (found !== target) {
      console.error(`DIVERSA ${loc.label}: ${found} (attesa ${target})`);
      bad++;
    } else {
      console.log(`ok     ${loc.label}: ${found}`);
    }
  }
  if (bad) {
    console.error(`\n${bad} discrepanze. Allinea con: node scripts/version.mjs sync ${target}`);
    process.exit(1);
  }
  console.log(`\nTutto a ${target}.`);
  process.exit(0);
}

// sync: per file, cosi' Cargo.lock si legge e scrive una volta sola.
const byFile = new Map();
for (const loc of locs) {
  if (!existsSync(loc.file)) continue;
  if (!byFile.has(loc.file)) byFile.set(loc.file, []);
  byFile.get(loc.file).push(loc);
}
for (const [file, group] of byFile) {
  const before = read(file);
  let src = before;
  for (const loc of group) {
    const found = loc.get(src);
    if (found == null) {
      if (!loc.optional) console.warn(`attenzione: ${loc.label}: versione non trovata, salto`);
      continue;
    }
    src = loc.set(src, target);
    console.log(`${found === target ? "=" : "→"} ${loc.label}: ${found} → ${target}`);
  }
  if (src !== before) writeFileSync(file, src);
}
console.log(`\nVersione ${target} scritta. Controllo: node scripts/version.mjs check`);
