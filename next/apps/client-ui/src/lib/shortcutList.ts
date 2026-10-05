/**
 * Global keyboard shortcuts: one table drives both the handler in AppShell and
 * the list shown in Settings, so they can't drift apart.
 */

export type ShortcutKey = {
  text: string;
  size?: "solo" | "wide" | "default";
};

export type ShortcutItem = {
  id: string;
  keys: ShortcutKey[];
  keySep?: string;
  description: string;
};

export type ShortcutAction =
  | "search"
  | "play"
  | "seekBack"
  | "seekForward"
  | "listen"
  | "shuffle"
  | "plectr";

/** Seconds jumped by ← / →. */
export const SHORTCUT_SEEK_SECONDS = 15;

/** Minimal element shape, so the logic is testable without a DOM. */
export type ShortcutTarget = {
  tagName?: string;
  isContentEditable?: boolean;
  getAttribute?: (name: string) => string | null;
  hasAttribute?: (name: string) => boolean;
  closest?: (selector: string) => unknown;
} | null;

export type ShortcutKeyEvent = {
  key: string;
  code?: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  repeat?: boolean;
  isComposing?: boolean;
  defaultPrevented?: boolean;
  target: unknown;
};

const TEXT_INPUT_TYPES = new Set([
  "",
  "text",
  "search",
  "email",
  "url",
  "tel",
  "password",
  "number",
  "date",
  "time",
  "datetime-local",
  "month",
  "week",
]);

/**
 * Widgets that use every key themselves (typing, arrows, Space): no shortcut
 * at all while focus is inside one.
 */
const KEY_CONSUMING_ROLES =
  "[role='slider'],[role='textbox'],[role='searchbox'],[role='combobox'],[role='listbox']," +
  "[role='spinbutton'],[role='menu'],[role='menubar'],[role='grid'],[role='tree']," +
  "[role='radiogroup'],[role='tablist'],[role='dialog'],[role='alertdialog'],dialog," +
  "[aria-modal='true'],[contenteditable]:not([contenteditable='false'])";

/** Controls activated by Space / Enter / arrows: those keys stay theirs. */
const ACTIVATABLE =
  "button,a[href],summary,audio,video,[role='button'],[role='link'],[role='checkbox']," +
  "[role='switch'],[role='radio'],[role='tab'],[role='option'],[role='menuitem']," +
  "[role='gridcell'],[tabindex]:not([tabindex='-1'])";

function asTarget(t: unknown): ShortcutTarget {
  return t && typeof t === "object" ? (t as ShortcutTarget) : null;
}

/** Text entry: typing must never trigger a shortcut. */
export function isTextEntry(target: unknown): boolean {
  const el = asTarget(target);
  if (!el?.tagName) return false;
  const tag = el.tagName.toUpperCase();
  if (tag === "TEXTAREA" || tag === "SELECT") return true;
  if (tag === "INPUT") {
    const type = (el.getAttribute?.("type") ?? "").toLowerCase();
    return TEXT_INPUT_TYPES.has(type);
  }
  return !!el.isContentEditable;
}

function inKeyConsumingWidget(el: ShortcutTarget): boolean {
  if (!el) return false;
  const tag = el.tagName?.toUpperCase();
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;
  if (el.isContentEditable) return true;
  return !!el.closest?.(KEY_CONSUMING_ROLES);
}

function onActivatable(el: ShortcutTarget): boolean {
  return !!el?.closest?.(ACTIVATABLE);
}

/**
 * Mark a surface that owns the keyboard (a running Plectr game, a fullscreen
 * visualizer…) with this attribute: while it is in the DOM global shortcuts
 * stay off.
 */
export const SHORTCUTS_OFF_ATTR = "data-rk-shortcuts-off";

/**
 * Which shortcut (if any) this keydown is. `modalOpen`: a dialog is on screen;
 * `suspended`: something marked `SHORTCUTS_OFF_ATTR` owns the keyboard.
 */
export function shortcutActionFor(
  e: ShortcutKeyEvent,
  opts: { modalOpen?: boolean; suspended?: boolean } = {},
): ShortcutAction | null {
  if (e.defaultPrevented || e.isComposing) return null;
  if (opts.modalOpen || opts.suspended) return null;
  const el = asTarget(e.target);
  if (inKeyConsumingWidget(el)) return null;

  const key = e.key;
  const lower = key.length === 1 ? key.toLowerCase() : key;
  const ctrlOrMeta = e.ctrlKey || e.metaKey;

  // The only combo: Ctrl/⌘ + K. Anything else with a modifier belongs to the
  // browser / OS (Ctrl+S, Alt+←, ⌘+P…).
  if (ctrlOrMeta && !e.altKey && !e.shiftKey && lower === "k") return "search";
  if (ctrlOrMeta || e.altKey) return null;

  // "/" needs Shift on several layouts (Italian: Shift+7): allow it there only.
  if (key === "/") return "search";
  if (e.shiftKey) return null;

  const isSpace = key === " " || e.code === "Space";
  const isArrow = key === "ArrowLeft" || key === "ArrowRight";
  if ((isSpace || isArrow) && onActivatable(el)) return null;

  if (isSpace) return e.repeat ? null : "play";
  if (key === "ArrowLeft") return "seekBack";
  if (key === "ArrowRight") return "seekForward";
  if (e.repeat) return null;
  if (lower === "i") return "listen";
  if (lower === "s") return "shuffle";
  if (lower === "p") return "plectr";
  return null;
}

/** Rows for Settings → Shortcuts, in the same order as the table above. */
export function shortcutItems(t: (key: string) => string): ShortcutItem[] {
  return [
    {
      id: "search",
      keys: [{ text: "/", size: "solo" }, { text: t("settings.kbdCtrlK") }],
      keySep: t("settings.shortcutOr"),
      description: t("settings.shortcutSearchDesc"),
    },
    {
      id: "play",
      keys: [{ text: t("settings.kbdSpace"), size: "wide" }],
      description: t("settings.shortcutPlayDesc"),
    },
    {
      id: "seek",
      keys: [
        { text: t("settings.kbdArrowLeft"), size: "solo" },
        { text: t("settings.kbdArrowRight"), size: "solo" },
      ],
      keySep: "/",
      description: t("settings.shortcutSeekDesc"),
    },
    {
      id: "listen",
      keys: [{ text: t("settings.kbdI"), size: "solo" }],
      description: t("settings.shortcutListenDesc"),
    },
    {
      id: "shuffle",
      keys: [{ text: "S", size: "solo" }],
      description: t("ui.shortcut.shuffleDesc"),
    },
    {
      id: "plectr",
      keys: [{ text: "P", size: "solo" }],
      description: t("ui.shortcut.plectrDesc"),
    },
  ];
}
