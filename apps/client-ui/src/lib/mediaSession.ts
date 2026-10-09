/**
 * OS media controls (lock screen, notification shade, keyboard media keys,
 * desktop widgets).
 *
 * Handlers are registered once against a bridge the player fills in, so the
 * OS keeps working across track changes without re-binding on every load.
 * Shuffle, repeat and favourite are not in the Media Session spec yet: browsers
 * that know those action names get them, the rest silently ignore the attempt.
 *
 * The Android WebView has no Media Session API at all, so the same three
 * setters also feed the native shell through `nativeMedia.ts`: one state, two
 * possible listeners.
 */

import { coverUrlFor, type CoverSize } from "./api";
import {
  pushDesktopMetadata,
  pushDesktopPlaybackState,
  pushDesktopPosition,
} from "./desktopMedia";
import { positionChanged, type SentPosition } from "./mediaPosition";
import { platformCaps } from "./platformCaps";
import {
  pushNativeMetadata,
  pushNativePlaybackState,
  pushNativePosition,
} from "./nativeMedia";

export type MediaSessionPlaybackState = "none" | "paused" | "playing";

export type MediaSessionTrack = {
  title: string;
  artist: string;
  album: string;
  albumId: number | null;
  /** `false` when the hub says the album has no artwork (no request then). */
  hasCover?: boolean | null;
  /** Cache-busting version of the artwork, when the hub sends one. */
  coverVersion?: string | number | null;
  /** Artwork outside the album covers (podcast episodes): wins when set. */
  artworkUrl?: string | null;
};

/** Artwork URL of a track's album, or null when there is none to ask for. */
function trackCover(track: MediaSessionTrack | null, size: CoverSize): string | null {
  if (track?.artworkUrl) return track.artworkUrl;
  if (!track || track.albumId == null) return null;
  return coverUrlFor(
    {
      rel_path: "",
      album_id: track.albumId,
      has_cover: track.hasCover ?? undefined,
      cover_version: track.coverVersion ?? null,
    },
    size,
  );
}

export type MediaSessionBridge = {
  play: () => void;
  pause: () => void;
  next: () => void;
  prev: () => void;
  seek: (timeSec: number) => void;
  seekBy: (deltaSec: number) => void;
  toggleShuffle: () => void;
  cycleRepeat: () => void;
  toggleFavorite: () => void;
  toggleExclude: () => void;
};

/**
 * Real variants the hub can serve: the two cached thumbnails plus the original.
 * Listing the same bytes under six invented sizes, as 5.x did, only makes the OS
 * pick a 10 KB thumbnail for a full-screen lock screen. The original is declared
 * as 512 because that is the largest slot the OS asks for.
 */
const ARTWORK_VARIANTS = [
  { size: 128 as const, sizes: "128x128" },
  { size: 256 as const, sizes: "256x256" },
  { size: "full" as const, sizes: "512x512" },
];

function canUseMediaSession(): boolean {
  return typeof navigator !== "undefined" && "mediaSession" in navigator;
}

function absolute(url: string): string {
  if (/^https?:\/\//i.test(url)) return url;
  if (typeof location === "undefined") return url;
  return new URL(url, location.origin).href;
}

/**
 * WebKitGTK (the Linux shell) publishes the media session as an MPRIS player:
 * the desktop shell's media widget loads the artwork itself, and with the
 * original declared it decoded a multi-megabyte, 3000 px image on the
 * compositor's thread for a 64 px square. There only the cached 256 px
 * thumbnail is declared.
 */
function artworkVariants(): typeof ARTWORK_VARIANTS {
  return platformCaps.webkitGtk ? ARTWORK_VARIANTS.filter((v) => v.size === 256) : ARTWORK_VARIANTS;
}

export function buildMediaSessionArtwork(
  albumId: number | null,
  cover?: { hasCover?: boolean | null; coverVersion?: string | number | null },
): MediaImage[] {
  if (albumId == null) return [];
  const images: MediaImage[] = [];
  for (const variant of artworkVariants()) {
    const url = trackCover(
      { title: "", artist: "", album: "", albumId, ...cover },
      variant.size,
    );
    if (!url) return [];
    images.push({ src: absolute(url), sizes: variant.sizes, type: "image/jpeg" });
  }
  return images;
}

function metadataKey(track: MediaSessionTrack): string {
  return [
    track.title,
    track.artist,
    track.album,
    track.albumId ?? "-",
    track.hasCover === false ? "0" : "1",
    track.coverVersion ?? "",
    track.artworkUrl ?? "",
  ].join("\u0000");
}

let lastMetadataKey: string | null = null;

export function setMediaSessionMetadata(track: MediaSessionTrack | null): void {
  // The 256 thumbnail is the one the hub keeps in cache and it is the size
  // the Android notification shows: asking for the original would mean downloading
  // a few megabytes for a small square.
  const shade = trackCover(track, 256);
  pushNativeMetadata(track, shade ? absolute(shade) : "");
  pushDesktopMetadata(track, shade ? absolute(shade) : "");
  if (!canUseMediaSession()) return;
  if (!track) {
    lastMetadataKey = null;
    navigator.mediaSession.metadata = null;
    return;
  }
  const key = metadataKey(track);
  if (key === lastMetadataKey) return;
  lastMetadataKey = key;
  const artwork = track.artworkUrl
    ? [{ src: absolute(track.artworkUrl), sizes: "400x400", type: "image/jpeg" }]
    : buildMediaSessionArtwork(track.albumId, track);
  navigator.mediaSession.metadata = new MediaMetadata({
    title: track.title,
    artist: track.artist,
    album: track.album,
    artwork,
  });
  // Warm the size the shade usually shows, so the first paint is not blank.
  const shadeArt = artwork.find((a) => a.sizes === "256x256" || a.sizes === "400x400");
  if (typeof Image !== "undefined" && shadeArt) {
    const warm = new Image();
    warm.decoding = "async";
    warm.src = shadeArt.src;
  }
}

let lastPlaybackState: MediaSessionPlaybackState | null = null;

/**
 * Every write reaches the OS (MPRIS on Linux: a D-Bus signal the desktop
 * shell re-renders on), so only real changes are sent.
 */
export function setMediaSessionPlaybackState(
  state: MediaSessionPlaybackState,
): void {
  pushNativePlaybackState(state);
  pushDesktopPlaybackState(state);
  if (!canUseMediaSession()) return;
  if (state === lastPlaybackState) return;
  lastPlaybackState = state;
  if (state === "none") lastPosition = null;
  navigator.mediaSession.playbackState = state;
}

let lastPosition: SentPosition | null = null;

export function setMediaSessionPosition(
  duration: number,
  position: number,
  playbackRate = 1,
): void {
  pushNativePosition(duration, position);
  pushDesktopPosition(duration, position);
  if (!canUseMediaSession()) return;
  if (!("setPositionState" in navigator.mediaSession)) return;
  if (!Number.isFinite(duration) || duration <= 0) return;
  const next: SentPosition = {
    duration,
    position: Math.max(0, Math.min(position, duration)),
    rate: playbackRate > 0 ? playbackRate : 1,
    at: Date.now(),
    playing: lastPlaybackState === "playing",
  };
  if (!positionChanged(lastPosition, next)) return;
  lastPosition = next;
  try {
    navigator.mediaSession.setPositionState({
      duration: next.duration,
      playbackRate: next.rate,
      position: next.position,
    });
  } catch {
    /* Safari throws on out-of-range values instead of clamping. */
  }
}

export function clearMediaSessionPosition(): void {
  lastPosition = null;
  if (!canUseMediaSession()) return;
  if (!("setPositionState" in navigator.mediaSession)) return;
  try {
    navigator.mediaSession.setPositionState();
  } catch {
    /* */
  }
}

/** Action names shipped by some browsers ahead of the spec, per feature. */
const EXPERIMENTAL_ACTIONS = {
  shuffle: ["toggleshuffle", "shuffle"],
  repeat: ["switchrepeatmode", "togglerepeat", "setrepeatmode", "repeat"],
  favorite: ["togglelike", "togglefavorite", "like", "favorite"],
  exclude: ["toggleshuffleexclude", "toggleexcludetrack", "dislike"],
} as const;

const STANDARD_ACTIONS: MediaSessionAction[] = [
  "play",
  "pause",
  "stop",
  "previoustrack",
  "nexttrack",
  "seekto",
  "seekbackward",
  "seekforward",
];

/**
 * Shells that own the real media notification (Tauri, Android WebView) have no
 * Media Session API of their own: they dispatch this event to reach the same
 * bridge, including the actions no browser exposes yet.
 */
export const MEDIA_ACTION_EVENT = "rekord:media-action";

export type MediaActionDetail = {
  action: string;
  /** Seconds for `seekto` / `seekby`; ignored elsewhere. */
  value?: number;
};

function applyMediaAction(
  bridge: MediaSessionBridge,
  action: string,
  value?: number,
): void {
  switch (action) {
    case "play":
      bridge.play();
      return;
    case "pause":
    case "stop":
      bridge.pause();
      return;
    case "nexttrack":
      bridge.next();
      return;
    case "previoustrack":
      bridge.prev();
      return;
    case "seekto":
      if (value != null && Number.isFinite(value)) bridge.seek(value);
      return;
    case "seekby":
      if (value != null && Number.isFinite(value)) bridge.seekBy(value);
      return;
    default:
      break;
  }
  if ((EXPERIMENTAL_ACTIONS.shuffle as readonly string[]).includes(action)) {
    bridge.toggleShuffle();
  } else if ((EXPERIMENTAL_ACTIONS.repeat as readonly string[]).includes(action)) {
    bridge.cycleRepeat();
  } else if (
    (EXPERIMENTAL_ACTIONS.favorite as readonly string[]).includes(action)
  ) {
    bridge.toggleFavorite();
  } else if (
    (EXPERIMENTAL_ACTIONS.exclude as readonly string[]).includes(action)
  ) {
    bridge.toggleExclude();
  }
}

function trySetHandler(
  action: string,
  handler: MediaSessionActionHandler | null,
): void {
  try {
    navigator.mediaSession.setActionHandler(
      action as MediaSessionAction,
      handler,
    );
  } catch {
    /* Unknown action for this browser — nothing to fall back to. */
  }
}

export function registerMediaSessionActions(
  getBridge: () => MediaSessionBridge,
): () => void {
  // A throwing handler makes some browsers drop the whole session.
  const dispatch = (action: string, value?: number) => {
    try {
      applyMediaAction(getBridge(), action, value);
    } catch {
      /* */
    }
  };

  const onShellAction = (event: Event) => {
    const detail = (event as CustomEvent<MediaActionDetail>).detail;
    if (!detail?.action) return;
    dispatch(detail.action, detail.value);
  };
  if (typeof window !== "undefined") {
    window.addEventListener(MEDIA_ACTION_EVENT, onShellAction);
  }
  const removeShellListener = () => {
    if (typeof window !== "undefined") {
      window.removeEventListener(MEDIA_ACTION_EVENT, onShellAction);
    }
  };

  if (!canUseMediaSession()) return removeShellListener;

  for (const action of ["play", "pause", "stop", "previoustrack", "nexttrack"]) {
    trySetHandler(action, () => dispatch(action));
  }
  trySetHandler("seekto", (details) => {
    if (details.seekTime == null) return;
    dispatch("seekto", details.seekTime);
  });
  trySetHandler("seekbackward", (details) =>
    dispatch("seekby", -(details.seekOffset ?? 10)),
  );
  trySetHandler("seekforward", (details) =>
    dispatch("seekby", details.seekOffset ?? 10),
  );
  // Ignored by every desktop browser today; the ones that ship them get them.
  for (const actions of Object.values(EXPERIMENTAL_ACTIONS)) {
    for (const action of actions) trySetHandler(action, () => dispatch(action));
  }

  return () => {
    removeShellListener();
    for (const action of STANDARD_ACTIONS) trySetHandler(action, null);
    for (const actions of Object.values(EXPERIMENTAL_ACTIONS)) {
      for (const action of actions) trySetHandler(action, null);
    }
  };
}
