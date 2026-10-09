<script lang="ts">
  import { Field, Select } from "@rekord/ui";
  import { t } from "../../lib/i18n.svelte";
  import {
    applyLayoutWidth,
    clampLayoutWidth,
    DEFAULT_CONTENT_MAX_PX,
    DEFAULT_DOCK_MAX_PX,
    LAYOUT_WIDTH_MAX,
    LAYOUT_WIDTH_MIN,
    LAYOUT_WIDTH_PRESETS,
    LAYOUT_WIDTH_STEP,
    loadLayoutWidth,
    saveLayoutWidth,
    type ContentWidthPref,
    type DockWidthPref,
    type LayoutWidthPrefs,
  } from "../../lib/layoutWidth";

  /** Desktop widths, per device: Settings › Interface. */
  const initial = loadLayoutWidth();
  let prefs = $state<LayoutWidthPrefs>(initial);

  const PRESET_SET = new Set<number>(LAYOUT_WIDTH_PRESETS);

  function choiceOf(v: ContentWidthPref | DockWidthPref): string {
    if (typeof v === "number") return PRESET_SET.has(v) ? String(v) : "custom";
    return v;
  }

  const presetOptions = $derived(
    LAYOUT_WIDTH_PRESETS.map((px) => ({ value: String(px), label: `${px} px` })),
  );

  const contentOptions = $derived([
    {
      value: "default",
      label: t("appearance.width.default", { px: DEFAULT_CONTENT_MAX_PX }),
    },
    { value: "full", label: t("appearance.width.full") },
    ...presetOptions,
    { value: "custom", label: t("appearance.width.custom") },
  ]);

  const dockOptions = $derived([
    { value: "default", label: t("appearance.width.default", { px: DEFAULT_DOCK_MAX_PX }) },
    { value: "full", label: t("appearance.width.full") },
    { value: "content", label: t("appearance.width.matchContent") },
    ...presetOptions,
    { value: "custom", label: t("appearance.width.custom") },
  ]);

  let contentChoice = $state(choiceOf(initial.content));
  let dockChoice = $state(choiceOf(initial.dock));

  /** Slider value when the choice is not already a number. */
  function startWidth(v: ContentWidthPref | DockWidthPref, fallback: number): number {
    if (typeof v === "number") return v;
    if (v === "full" && typeof window !== "undefined") return clampLayoutWidth(window.innerWidth);
    return clampLayoutWidth(fallback);
  }

  function commit(next: LayoutWidthPrefs) {
    prefs = next;
    applyLayoutWidth(next);
    saveLayoutWidth(next);
  }

  function onContentChoice(value: string) {
    contentChoice = value;
    const content: ContentWidthPref =
      value === "default" || value === "full"
        ? value
        : value === "custom"
          ? startWidth(prefs.content, DEFAULT_CONTENT_MAX_PX)
          : clampLayoutWidth(Number(value));
    commit({ ...prefs, content });
  }

  function onDockChoice(value: string) {
    dockChoice = value;
    const dock: DockWidthPref =
      value === "default" || value === "full" || value === "content"
        ? value
        : value === "custom"
          ? startWidth(prefs.dock, DEFAULT_DOCK_MAX_PX)
          : clampLayoutWidth(Number(value));
    commit({ ...prefs, dock });
  }

  /* Dragging: live preview through the CSS variables, saved on release. */
  function previewContent(px: number) {
    prefs = { ...prefs, content: clampLayoutWidth(px) };
    applyLayoutWidth(prefs);
  }

  function previewDock(px: number) {
    prefs = { ...prefs, dock: clampLayoutWidth(px) };
    applyLayoutWidth(prefs);
  }

  function save() {
    saveLayoutWidth(prefs);
  }

  function selectValue(ev: Event): string {
    return (ev.currentTarget as HTMLSelectElement).value;
  }

  function rangeValue(ev: Event): number {
    return Number((ev.currentTarget as HTMLInputElement).value);
  }
</script>

<div class="settings-fields-grid__span layout-width">
  <div class="layout-width__fields">
    <div class="layout-width__field">
      <Field label={t("appearance.width.content")}>
        <Select
          options={contentOptions}
          value={contentChoice}
          aria-label={t("appearance.width.content")}
          onchange={(ev) => onContentChoice(selectValue(ev))}
        />
      </Field>
      {#if contentChoice === "custom" && typeof prefs.content === "number"}
        <div class="settings-glass-opacity layout-width__slider">
          <input
            type="range"
            class="settings-glass-opacity__slider"
            min={LAYOUT_WIDTH_MIN}
            max={LAYOUT_WIDTH_MAX}
            step={LAYOUT_WIDTH_STEP}
            value={prefs.content}
            aria-label={t("appearance.width.contentCustomAria")}
            oninput={(e) => previewContent(rangeValue(e))}
            onchange={save}
          />
          <span class="layout-width__px">{prefs.content} px</span>
        </div>
      {/if}
    </div>
    <div class="layout-width__field">
      <Field label={t("appearance.width.dock")}>
        <Select
          options={dockOptions}
          value={dockChoice}
          aria-label={t("appearance.width.dock")}
          onchange={(ev) => onDockChoice(selectValue(ev))}
        />
      </Field>
      {#if dockChoice === "custom" && typeof prefs.dock === "number"}
        <div class="settings-glass-opacity layout-width__slider">
          <input
            type="range"
            class="settings-glass-opacity__slider"
            min={LAYOUT_WIDTH_MIN}
            max={LAYOUT_WIDTH_MAX}
            step={LAYOUT_WIDTH_STEP}
            value={prefs.dock}
            aria-label={t("appearance.width.dockCustomAria")}
            oninput={(e) => previewDock(rangeValue(e))}
            onchange={save}
          />
          <span class="layout-width__px">{prefs.dock} px</span>
        </div>
      {/if}
    </div>
  </div>
  <p class="layout-width__hint">{t("appearance.width.hint")}</p>
</div>

<style>
  .layout-width {
    display: grid;
    gap: 0.45rem;
    min-width: 0;
  }

  .layout-width__fields {
    display: grid;
    gap: 0.85rem 1rem;
    grid-template-columns: minmax(0, 1fr);
    min-width: 0;
  }

  @media (min-width: 720px) {
    .layout-width__fields {
      grid-template-columns: repeat(2, minmax(0, 1fr));
      align-items: start;
    }
  }

  .layout-width__field {
    display: grid;
    gap: 0.4rem;
    min-width: 0;
  }

  .layout-width__field :global(.rk-field) {
    margin-bottom: 0;
  }

  .layout-width__slider {
    grid-template-columns: minmax(0, 1fr) auto;
    grid-template-rows: auto;
  }

  .layout-width__slider .settings-glass-opacity__slider {
    grid-row: 1;
  }

  .layout-width__px {
    grid-column: 2;
    grid-row: 1;
    min-width: 4.5rem;
    text-align: right;
    font-size: var(--rk-fs-sm);
    font-variant-numeric: tabular-nums;
    color: var(--rk-muted-strong);
  }

  /* Phones and tablets in the mobile layout: the settings do nothing there. */
  @media (max-width: 999.98px) and (pointer: coarse) {
    .layout-width {
      display: none;
    }
  }

  .layout-width__hint {
    margin: 0;
    font-size: var(--rk-fs-sm);
    color: var(--rk-muted);
  }
</style>
