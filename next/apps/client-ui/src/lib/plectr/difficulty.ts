import type { DifficultyId } from "./types";

/** Legacy React client key (global, not per account). */
export const LEGACY_PLECTR_DIFFICULTY_KEY = "rekord-plectr-difficulty";

const VALID: readonly string[] = ["easy", "normal", "hard"];

export function isDifficultyId(raw: unknown): raw is DifficultyId {
  return typeof raw === "string" && VALID.includes(raw);
}

/** Legacy ids: `extreme` became `hard`; unknown values fall back to easy. */
export function migratePlectrPlayMode(raw: unknown): DifficultyId {
  if (raw === "extreme") return "hard";
  if (isDifficultyId(raw)) return raw;
  return "easy";
}

/** Difficulty saved by the legacy client on this device, if any. */
export function readLegacyPlectrPlayMode(): DifficultyId | null {
  try {
    const raw = localStorage.getItem(LEGACY_PLECTR_DIFFICULTY_KEY);
    return raw == null ? null : migratePlectrPlayMode(raw);
  } catch {
    return null;
  }
}
