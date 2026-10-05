/**
 * Tabelle di traduzione: base (`locales/{it,en}.json`) piu' i frammenti per area
 * (`locales/<area>/{it,en}.json`) che `i18n.svelte.ts` fonde all'avvio.
 *
 * - ogni tabella ha le stesse chiavi in italiano, inglese e tedesco (`de` segue
 *   `en`: stesse chiavi, stesse forme plurali `.one` / `.other`);
 * - nessuna chiave compare in due tabelle (la fusione ne terrebbe una a caso);
 * - i segnaposto `{{var}}` coincidono tra le lingue;
 * - ogni `t("chiave.letterale")` / `tp("chiave", n)` nel codice esiste davvero.
 *   Le chiavi costruite a runtime (template literal, variabili) si saltano.
 *
 * Si lancia con `pnpm test`.
 */
import assert from "node:assert/strict";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { describe, test } from "node:test";
import { fileURLToPath } from "node:url";
import { join, relative } from "node:path";

const SRC = fileURLToPath(new URL("..", import.meta.url));
const LOCALES = join(SRC, "locales");
const LANGS = ["it", "en", "de"];
/** Languages checked against the English table (key set and placeholders). */
const FOLLOW_EN = ["de"];

function readTable(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

/** `{ name: "base" | <area>, it: {...}, en: {...} }` for every table. */
function loadTables() {
  const tables = [{ name: "base", dir: LOCALES }];
  for (const entry of readdirSync(LOCALES)) {
    const dir = join(LOCALES, entry);
    if (statSync(dir).isDirectory()) tables.push({ name: entry, dir });
  }
  return tables.map(({ name, dir }) => {
    const out = { name };
    for (const lang of LANGS) out[lang] = readTable(join(dir, `${lang}.json`));
    return out;
  });
}

const TABLES = loadTables();

function placeholders(text) {
  return [...String(text).matchAll(/\{\{\s*(\w+)\s*\}\}/g)].map((m) => m[1]).sort();
}

describe("locale tables", () => {
  test("every table has an Italian, an English and a German file", () => {
    assert.ok(TABLES.length > 1, "base table plus at least one fragment");
    for (const table of TABLES) {
      for (const lang of LANGS) {
        assert.equal(typeof table[lang], "object", `${table.name}/${lang}.json`);
      }
    }
  });

  for (const table of TABLES) {
    test(`${table.name}: Italian and English have the same keys`, () => {
      const it = Object.keys(table.it).sort();
      const en = Object.keys(table.en).sort();
      const onlyIt = it.filter((k) => !(k in table.en));
      const onlyEn = en.filter((k) => !(k in table.it));
      assert.deepEqual(onlyIt, [], `${table.name}: keys missing in en.json`);
      assert.deepEqual(onlyEn, [], `${table.name}: keys missing in it.json`);
    });

    test(`${table.name}: values are non-empty strings`, () => {
      for (const lang of LANGS) {
        for (const [key, value] of Object.entries(table[lang])) {
          assert.equal(typeof value, "string", `${table.name}/${lang}: ${key}`);
          assert.ok(value.length > 0, `${table.name}/${lang}: ${key} is empty`);
        }
      }
    });

    test(`${table.name}: {{placeholders}} match between the languages`, () => {
      for (const key of Object.keys(table.it)) {
        if (!(key in table.en)) continue;
        assert.deepEqual(
          [...new Set(placeholders(table.en[key]))],
          [...new Set(placeholders(table.it[key]))],
          `${table.name}: placeholders differ for ${key}`,
        );
      }
    });

    for (const lang of FOLLOW_EN) {
      test(`${table.name}: ${lang}.json has exactly the English keys`, () => {
        const onlyEn = Object.keys(table.en).filter((k) => !(k in table[lang])).sort();
        const extra = Object.keys(table[lang]).filter((k) => !(k in table.en)).sort();
        assert.deepEqual(onlyEn, [], `${table.name}: keys missing in ${lang}.json`);
        assert.deepEqual(extra, [], `${table.name}: keys in ${lang}.json but not in en.json`);
      });

      test(`${table.name}: ${lang}.json keeps the English {{placeholders}}`, () => {
        for (const key of Object.keys(table.en)) {
          if (!(key in table[lang])) continue;
          assert.deepEqual(
            placeholders(table[lang][key]),
            placeholders(table.en[key]),
            `${table.name}/${lang}: placeholders differ for ${key}`,
          );
        }
      });
    }
  }

  test("no key is defined in two tables", () => {
    const owner = new Map();
    const dupes = [];
    for (const table of TABLES) {
      for (const key of Object.keys(table.it)) {
        const prev = owner.get(key);
        if (prev) dupes.push(`${key} (${prev} + ${table.name})`);
        else owner.set(key, table.name);
      }
    }
    assert.deepEqual(dupes, []);
  });
});

/* ── Static check: literal keys used in the code ── */

function walk(dir, out = []) {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) {
      if (entry !== "locales" && entry !== "node_modules") walk(path, out);
    } else if (/\.(svelte|ts)$/.test(entry) && !/\.d\.ts$/.test(entry)) {
      out.push(path);
    }
  }
  return out;
}

/** Comments may quote calls as examples: strip them before scanning. */
function stripComments(src) {
  return src
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/(^|[^:"'`\\])\/\/[^\n]*/g, "$1");
}

const CALL = /(?<![\w$.])(?:i18n\.)?(t|tp)\(\s*(["'])([^"'`\n]+?)\2/g;

describe("keys used in src", () => {
  const all = new Set();
  for (const table of TABLES) for (const key of Object.keys(table.it)) all.add(key);

  test("every t(\"literal\") / tp(\"literal\", n) key exists", () => {
    const missing = [];
    let checked = 0;
    for (const file of walk(SRC)) {
      const src = stripComments(readFileSync(file, "utf8"));
      for (const m of src.matchAll(CALL)) {
        const [, fn, , key] = m;
        if (!/^[\w-]+(\.[\w-]+)+$/.test(key)) continue; // not a key-shaped literal
        checked += 1;
        const ok = fn === "tp" ? all.has(`${key}.other`) : all.has(key);
        if (!ok) missing.push(`${relative(SRC, file)}: ${fn}("${key}")`);
      }
    }
    assert.ok(checked > 500, `expected many literal keys, found ${checked}`);
    assert.deepEqual(missing, []);
  });
});
