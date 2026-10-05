/**
 * Behaviour shared by every modal surface (the `Modal` component and the
 * app's own full-screen dialogs): a stack so Escape and Back close only the
 * top-most one, body scroll lock while any is open, focus moved inside on open
 * and given back on close, and Tab kept inside the top-most dialog.
 */

import { pushBackLayer } from "./backStack";

type Entry = {
  id: number;
  panel: () => HTMLElement | null;
  close: () => void;
};

const stack: Entry[] = [];
let nextId = 1;
let savedOverflow: string | null = null;
let keyListening = false;

const FOCUSABLE = [
  "a[href]",
  "area[href]",
  "button:not([disabled])",
  "input:not([disabled]):not([type='hidden'])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "iframe",
  "audio[controls]",
  "video[controls]",
  "[contenteditable]:not([contenteditable='false'])",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

export function focusableIn(root: HTMLElement): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE)).filter((el) => {
    if (el.closest("[inert]")) return false;
    if (el.getAttribute("aria-hidden") === "true") return false;
    // Hidden by layout (display:none ancestors, collapsed details…).
    return el.getClientRects().length > 0;
  });
}

function lockScroll() {
  if (savedOverflow !== null) return;
  savedOverflow = document.body.style.overflow;
  document.body.style.overflow = "hidden";
}

function unlockScroll() {
  if (savedOverflow === null) return;
  document.body.style.overflow = savedOverflow;
  savedOverflow = null;
}

function onKeyDown(e: KeyboardEvent) {
  const top = stack[stack.length - 1];
  if (!top) return;
  if (e.key === "Escape") {
    if (e.defaultPrevented) return;
    e.preventDefault();
    e.stopPropagation();
    top.close();
    return;
  }
  if (e.key !== "Tab") return;
  const panel = top.panel();
  if (!panel) return;
  const items = focusableIn(panel);
  if (!items.length) {
    e.preventDefault();
    panel.focus();
    return;
  }
  const first = items[0]!;
  const last = items[items.length - 1]!;
  const active = document.activeElement as HTMLElement | null;
  const inside = !!active && panel.contains(active);
  if (e.shiftKey && (!inside || active === first || active === panel)) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && (!inside || active === last)) {
    e.preventDefault();
    first.focus();
  }
}

/** True while at least one modal surface is open. */
export function isModalOpen(): boolean {
  if (stack.length) return true;
  // Dialogs that don't register (third-party / legacy markup) still count.
  return typeof document !== "undefined" && !!document.querySelector("[aria-modal='true']");
}

export type ModalRegistration = {
  /** Call on close; restores focus to `returnFocus` when it is still attached. */
  release: () => void;
};

export type ModalOptions = {
  /** The dialog panel (focus trap root). */
  panel: () => HTMLElement | null;
  /** Ask the owner to close (Escape on top-most, Android Back). */
  close: () => void;
  /** Element to focus on open; defaults to the first focusable / the panel. */
  initialFocus?: () => HTMLElement | null;
  /** Skip moving focus inside (e.g. phone sheet: avoid popping the keyboard). */
  focusPanelOnly?: boolean;
  /** Push a history entry so the hardware Back closes this first. Default true. */
  history?: boolean;
};

/**
 * Register an open modal. Call from an effect when the dialog opens and call
 * `release()` when it closes / unmounts.
 */
export function registerModal(opts: ModalOptions): ModalRegistration {
  const returnFocus =
    typeof document !== "undefined" ? (document.activeElement as HTMLElement | null) : null;
  let released = false;
  const entry: Entry = { id: nextId++, panel: opts.panel, close: opts.close };
  stack.push(entry);
  lockScroll();
  if (!keyListening) {
    // Capture: Escape inside a focused field still reaches the top-most dialog
    // before page handlers (and only that one).
    window.addEventListener("keydown", onKeyDown, true);
    keyListening = true;
  }

  let releaseBack: () => void = () => {};
  const armBack = () => {
    if (opts.history === false) return;
    releaseBack = pushBackLayer(() => {
      releaseBack = () => {};
      opts.close();
      // The owner may refuse (busy, nested dialog): keep Back working for it.
      setTimeout(() => {
        if (released) return;
        const panel = opts.panel();
        if (panel?.isConnected) armBack();
      }, 0);
    });
  };
  armBack();

  queueMicrotask(() => {
    if (released) return;
    const panel = opts.panel();
    if (!panel) return;
    if (panel.contains(document.activeElement)) return;
    const target =
      opts.initialFocus?.() ??
      (opts.focusPanelOnly
        ? null
        : (panel.querySelector<HTMLElement>("[autofocus],[data-autofocus]") ??
          focusableIn(panel).find((el) => !el.hasAttribute("data-modal-close")) ??
          null));
    (target ?? panel).focus({ preventScroll: true });
  });

  function remove() {
    const i = stack.indexOf(entry);
    if (i >= 0) stack.splice(i, 1);
    if (!stack.length) {
      unlockScroll();
      if (keyListening) {
        window.removeEventListener("keydown", onKeyDown, true);
        keyListening = false;
      }
    }
  }

  return {
    release() {
      if (!released) {
        released = true;
        remove();
        releaseBack();
      }
      if (returnFocus && returnFocus.isConnected && returnFocus !== document.body) {
        const active = document.activeElement;
        // Don't steal focus from something the user moved to meanwhile.
        if (!active || active === document.body || !active.isConnected || isInsideClosed(active)) {
          returnFocus.focus({ preventScroll: true });
        }
      }
    },
  };

  function isInsideClosed(el: Element): boolean {
    const panel = opts.panel();
    return !!panel && panel.contains(el);
  }
}

export type ModalSurfaceOptions = {
  onclose: () => void;
  focusPanelOnly?: boolean;
  history?: boolean;
};

/**
 * Svelte action for a dialog panel that is mounted only while open
 * (`{#if open}<div role="dialog" use:modalSurface={{ onclose }}>`).
 */
export function modalSurface(node: HTMLElement, options: ModalSurfaceOptions) {
  let opts = options;
  const reg = registerModal({
    panel: () => node,
    close: () => opts.onclose(),
    focusPanelOnly: opts.focusPanelOnly,
    history: opts.history,
  });
  return {
    update(next: ModalSurfaceOptions) {
      opts = next;
    },
    destroy() {
      reg.release();
    },
  };
}
