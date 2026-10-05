/**
 * Versioni `MAJOR.MINOR.PATCH[-pre][+build]`, quanto basta per confrontare il
 * client con l'hub (`/api/v1/health` → `version`, `minClientVersion`).
 *
 * Volutamente tollerante: un prefisso `v`, parti mancanti (`5.1` = `5.1.0`) e
 * spazi passano; una stringa che non somiglia a una versione torna `null`, e chi
 * chiama la tratta come «non so» invece di mostrare avvisi a caso.
 */

export type SemVer = {
  major: number;
  minor: number;
  patch: number;
  /** Identificatori del pre-release (`rc.1` → `["rc", "1"]`), vuoto per una release. */
  pre: string[];
};

const PATTERN =
  /^v?(\d{1,9})(?:\.(\d{1,9}))?(?:\.(\d{1,9}))?(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$/;

export function parseSemver(raw: unknown): SemVer | null {
  if (typeof raw !== "string") return null;
  const m = PATTERN.exec(raw.trim());
  if (!m) return null;
  return {
    major: Number(m[1]),
    minor: Number(m[2] ?? 0),
    patch: Number(m[3] ?? 0),
    pre: m[4] ? m[4].split(".").filter(Boolean) : [],
  };
}

function comparePre(a: string[], b: string[]): number {
  // Una release batte qualunque suo pre-release: 5.1.0 > 5.1.0-rc.1.
  if (!a.length && !b.length) return 0;
  if (!a.length) return 1;
  if (!b.length) return -1;
  const n = Math.max(a.length, b.length);
  for (let i = 0; i < n; i++) {
    const x = a[i];
    const y = b[i];
    if (x === undefined) return -1;
    if (y === undefined) return 1;
    const xn = /^\d+$/.test(x);
    const yn = /^\d+$/.test(y);
    if (xn && yn) {
      const d = Number(x) - Number(y);
      if (d) return d < 0 ? -1 : 1;
      continue;
    }
    // Numeri prima delle parole, come da semver.org.
    if (xn) return -1;
    if (yn) return 1;
    if (x !== y) return x < y ? -1 : 1;
  }
  return 0;
}

/** -1, 0, 1 come `a` rispetto a `b`; `null` se una delle due non e' leggibile. */
export function compareSemver(a: unknown, b: unknown): -1 | 0 | 1 | null {
  const x = typeof a === "object" && a !== null ? (a as SemVer) : parseSemver(a);
  const y = typeof b === "object" && b !== null ? (b as SemVer) : parseSemver(b);
  if (!x || !y) return null;
  for (const key of ["major", "minor", "patch"] as const) {
    if (x[key] !== y[key]) return x[key] < y[key] ? -1 : 1;
  }
  const p = comparePre(x.pre, y.pre);
  return p === 0 ? 0 : p < 0 ? -1 : 1;
}

/** Vero se `version` e' almeno `min`. Senza un minimo leggibile non c'e' vincolo. */
export function satisfiesMin(version: unknown, min: unknown): boolean {
  if (parseSemver(min) === null) return true;
  const c = compareSemver(version, min);
  return c === null ? true : c >= 0;
}

/** Quanto e' grande la distanza: serve a non disturbare per una patch. */
export function diffLevel(a: unknown, b: unknown): "major" | "minor" | "patch" | "pre" | null {
  const x = parseSemver(a);
  const y = parseSemver(b);
  if (!x || !y) return null;
  if (x.major !== y.major) return "major";
  if (x.minor !== y.minor) return "minor";
  if (x.patch !== y.patch) return "patch";
  if (comparePre(x.pre, y.pre) !== 0) return "pre";
  return null;
}
