import { api, type Track } from "./api";
import { lyricsKind } from "./trackMoods";

export type AutoLrcStatus = "okLrc" | "okPlain" | "missing";

export type AutoLrcResult = {
  status: AutoLrcStatus;
  lyrics: string | null;
};

/**
 * Auto LRC quick-save: fetch from LRCLIB and save straight onto the track
 * (legacy parity `runAutoLrcQuickSaveForTrack`).
 */
export async function runAutoLrcQuickSaveForTrack(track: Track): Promise<AutoLrcResult> {
  const fetched = await api.trackLyricsFetch(track.rel_path);
  const synced = String(fetched.syncedLyrics || "").trim();
  const plain = String(fetched.plainLyrics || "").trim();
  const next = synced || plain;
  if (!next) {
    return { status: "missing", lyrics: null };
  }
  await api.trackInfoSave(track.rel_path, { lyrics: next });
  track.lyrics = next;
  const status: AutoLrcStatus =
    synced || lyricsKind(next) === "lrc" ? "okLrc" : "okPlain";
  return { status, lyrics: next };
}
