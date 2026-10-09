/**
 * Meta line of Studio › Listen ("Artist · Album · ♪ · 3:21 · (12)"): only the
 * parts that exist, so a missing album, no lyrics or no duration never leave
 * a doubled "· ·" separator behind.
 */
export type ListenMetaPart =
  | { kind: "names"; text: string }
  | { kind: "lyrics"; lyrics: "plain" | "lrc" }
  | { kind: "duration"; text: string }
  | { kind: "plays"; count: number };

export function listenMetaParts(input: {
  artist?: string | null;
  album?: string | null;
  lyrics: "off" | "plain" | "lrc";
  duration?: string | null;
  /** `null` hides the play count (external items such as podcast episodes). */
  plays: number | null;
}): ListenMetaPart[] {
  const parts: ListenMetaPart[] = [];
  const names = [input.artist, input.album]
    .map((s) => (s ?? "").trim())
    .filter(Boolean)
    .join(" · ");
  if (names) parts.push({ kind: "names", text: names });
  if (input.lyrics !== "off") parts.push({ kind: "lyrics", lyrics: input.lyrics });
  const duration = (input.duration ?? "").trim();
  if (duration) parts.push({ kind: "duration", text: duration });
  if (input.plays !== null) parts.push({ kind: "plays", count: input.plays });
  return parts;
}
