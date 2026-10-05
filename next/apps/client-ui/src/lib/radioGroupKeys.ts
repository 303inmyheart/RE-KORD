/**
 * Keyboard for a `role="radiogroup"` of `role="radio"` buttons (WAI-ARIA
 * radio pattern): arrows move the selection (and focus) to the next / previous
 * enabled option, wrapping; Home / End jump to the ends. Only the checked
 * option is in the Tab order. Selecting clicks the option, so the buttons keep
 * their own `onclick`.
 *
 *   <div role="radiogroup" use:radioGroupKeys>…</div>
 */
const STEP: Record<string, number> = {
  ArrowRight: 1,
  ArrowDown: 1,
  ArrowLeft: -1,
  ArrowUp: -1,
};

function radiosIn(node: HTMLElement): HTMLElement[] {
  return [...node.querySelectorAll<HTMLElement>('[role="radio"]')];
}

function enabled(el: HTMLElement): boolean {
  return !(el as HTMLButtonElement).disabled && el.getAttribute("aria-disabled") !== "true";
}

export function radioGroupKeys(node: HTMLElement) {
  const sync = () => {
    const all = radiosIn(node);
    const usable = all.filter(enabled);
    const current =
      usable.find((el) => el.getAttribute("aria-checked") === "true") ?? usable[0] ?? null;
    for (const el of all) {
      const want = el === current ? 0 : -1;
      if (el.tabIndex !== want) el.tabIndex = want;
    }
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    const step = STEP[event.key];
    if (step == null && event.key !== "Home" && event.key !== "End") return;
    const usable = radiosIn(node).filter(enabled);
    if (usable.length === 0) return;
    const focused = usable.indexOf(document.activeElement as HTMLElement);
    const checked = usable.findIndex((el) => el.getAttribute("aria-checked") === "true");
    const from = focused >= 0 ? focused : Math.max(0, checked);
    const to =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? usable.length - 1
          : (from + step! + usable.length) % usable.length;
    event.preventDefault();
    event.stopPropagation();
    const next = usable[to]!;
    next.focus();
    if (next.getAttribute("aria-checked") !== "true") next.click();
  };

  node.addEventListener("keydown", onKeyDown);
  const observer = new MutationObserver(sync);
  observer.observe(node, {
    subtree: true,
    childList: true,
    attributes: true,
    attributeFilter: ["aria-checked", "disabled", "aria-disabled"],
  });
  sync();

  return {
    destroy() {
      node.removeEventListener("keydown", onKeyDown);
      observer.disconnect();
    },
  };
}
