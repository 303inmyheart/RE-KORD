/**
 * `use:floating={{ anchor }}` — lift a dropdown / popover out of its card and
 * pin it to the viewport next to its anchor (legacy `createPortal` +
 * `usePopoverLayerAnchored`).
 *
 * Track rows and cards create their own stacking contexts (`contain`,
 * `isolation`, `transform`, Glass `backdrop-filter`) and some clip their
 * overflow, so a menu rendered inside them slides *under* the next row or gets
 * cut off whatever its z-index. Moved to `<body>` with `position: fixed`, the
 * menu sits above everything but dialogs' own toasts, and follows its anchor on
 * scroll and resize. It opens below the anchor and flips above when there is
 * more room there; its height is capped to the room it has (it scrolls inside).
 */

export type FloatingPlacement = "bottom-start" | "bottom-end" | "top-start" | "top-end";

export type FloatingOptions = {
  /** Element the popover hangs from (the button that opened it). */
  anchor: HTMLElement | null | undefined;
  /** Preferred side; flips vertically when the other side has more room. */
  placement?: FloatingPlacement;
  /** Gap between anchor and popover (px). */
  offset?: number;
  /** Minimum width: the anchor's width, or at least this many px. */
  minWidth?: number;
};

const EDGE = 8;

export function placeFloating(
  rect: Pick<DOMRect, "top" | "bottom" | "left" | "right" | "width">,
  size: { width: number; height: number },
  viewport: { width: number; height: number },
  placement: FloatingPlacement = "bottom-start",
  offset = 4,
): { top: number; left: number; maxHeight: number; side: "top" | "bottom" } {
  const below = viewport.height - rect.bottom - offset - EDGE;
  const above = rect.top - offset - EDGE;
  const wantTop = placement.startsWith("top");
  let side: "top" | "bottom" = wantTop ? "top" : "bottom";
  if (side === "bottom" && size.height > below && above > below) side = "top";
  else if (side === "top" && size.height > above && below > above) side = "bottom";
  const room = Math.max(80, side === "bottom" ? below : above);
  const height = Math.min(size.height, room);
  const top = side === "bottom" ? rect.bottom + offset : rect.top - offset - height;
  const alignEnd = placement.endsWith("end");
  let left = alignEnd ? rect.right - size.width : rect.left;
  left = Math.min(left, viewport.width - EDGE - size.width);
  left = Math.max(EDGE, left);
  return { top: Math.round(top), left: Math.round(left), maxHeight: Math.floor(room), side };
}

export function floating(node: HTMLElement, options: FloatingOptions) {
  let opts = options;
  let raf = 0;
  // Use it on the single root element of an `{#if}` block: Svelte then removes
  // exactly this node, wherever it lives.
  document.body.appendChild(node);
  node.classList.add("rk-floating");
  node.style.position = "fixed";
  node.style.zIndex = "calc(var(--rk-z-modal, 120) + 10)";
  node.style.margin = "0";
  node.style.right = "auto";
  node.style.bottom = "auto";

  const update = () => {
    raf = 0;
    const anchor = opts.anchor;
    if (!anchor || !anchor.isConnected) return;
    const rect = anchor.getBoundingClientRect();
    const minWidth = Math.max(rect.width, opts.minWidth ?? 0);
    node.style.minWidth = `${Math.round(minWidth)}px`;
    // Measure at natural height before capping it.
    node.style.maxHeight = "";
    // The stylesheet's own cap (e.g. 12.5rem for a long genre list) still wins.
    const cssMax = parseFloat(getComputedStyle(node).maxHeight);
    const size = {
      width: node.offsetWidth,
      height: Number.isFinite(cssMax) ? Math.min(node.scrollHeight, cssMax) : node.scrollHeight,
    };
    const viewport = { width: window.innerWidth, height: window.innerHeight };
    const pos = placeFloating(rect, size, viewport, opts.placement, opts.offset ?? 4);
    node.style.top = `${pos.top}px`;
    node.style.left = `${pos.left}px`;
    node.style.maxHeight = `${Math.min(pos.maxHeight, Number.isFinite(cssMax) ? cssMax : Infinity)}px`;
    node.dataset.side = pos.side;
  };
  const schedule = () => {
    if (!raf) raf = requestAnimationFrame(update);
  };

  update();
  window.addEventListener("scroll", schedule, true);
  window.addEventListener("resize", schedule);
  const ro = typeof ResizeObserver !== "undefined" ? new ResizeObserver(schedule) : null;
  ro?.observe(node);

  return {
    update(next: FloatingOptions) {
      opts = next;
      schedule();
    },
    destroy() {
      if (raf) cancelAnimationFrame(raf);
      window.removeEventListener("scroll", schedule, true);
      window.removeEventListener("resize", schedule);
      ro?.disconnect();
      // Normally gone already (Svelte removes the block's node first). When an
      // ancestor block is torn down instead, it only clears its own subtree:
      // never leave an orphan menu on <body>.
      if (node.parentNode === document.body) node.remove();
    },
  };
}
