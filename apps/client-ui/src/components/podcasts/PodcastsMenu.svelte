<script lang="ts">
  /** "⋯" of the podcasts page: the per-account "Show in Recent" switch. */
  import { floating } from "../../lib/floatingPopover";
  import { t } from "../../lib/i18n.svelte";
  import { podcasts } from "../../lib/podcasts/store.svelte";
  import UiIcon from "../icons/UiIcon.svelte";

  /** Also offer "Refresh" (phones, where the toolbar has no room for its button). */
  let { refreshItem = false }: { refreshItem?: boolean } = $props();

  let open = $state(false);
  let wrapEl = $state<HTMLDivElement | null>(null);
  let menuEl = $state<HTMLUListElement | null>(null);

  $effect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => {
      const target = e.target as Node | null;
      if (target && (wrapEl?.contains(target) || menuEl?.contains(target))) return;
      open = false;
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") open = false;
    };
    document.addEventListener("pointerdown", onDown, true);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onDown, true);
      document.removeEventListener("keydown", onKey);
    };
  });
</script>

<div class="podcasts-menu" bind:this={wrapEl}>
  <button
    type="button"
    class="podcasts-menu__btn"
    title={t("podcasts.options")}
    aria-label={t("podcasts.options")}
    aria-haspopup="menu"
    aria-expanded={open}
    onclick={() => (open = !open)}
  >
    <UiIcon name="more" />
  </button>
  {#if open}
    <ul
      bind:this={menuEl}
      class="track-row__overflow-menu rk-scroll"
      role="menu"
      use:floating={{ anchor: wrapEl, placement: "bottom-end", minWidth: 240 }}
    >
      {#if refreshItem}
        <li role="presentation" class="podcasts-menu__refresh">
          <button
            type="button"
            role="menuitem"
            class="track-row__overflow-item"
            disabled={podcasts.loading}
            onclick={() => {
              open = false;
              void podcasts.load({ force: true });
            }}
          >
            <span class="track-row__overflow-item-glyph track-row__ic-glyph--svg" aria-hidden="true"><UiIcon name="sync" /></span>
            <span class="track-row__overflow-item-label">{t("podcasts.refresh")}</span>
          </button>
        </li>
      {/if}
      <li role="presentation">
        <button
          type="button"
          role="menuitemcheckbox"
          class="track-row__overflow-item"
          class:is-on={podcasts.inRecent}
          aria-checked={podcasts.inRecent}
          title={t("podcasts.inRecentHint")}
          onclick={() => (podcasts.inRecent = !podcasts.inRecent)}
        >
          <span class="track-row__overflow-item-glyph track-row__ic-glyph--svg" aria-hidden="true"><UiIcon name="history" /></span>
          <span class="track-row__overflow-item-label">{t("podcasts.inRecent")}</span>
          <span class="podcasts-menu__check" aria-hidden="true">{#if podcasts.inRecent}<UiIcon name="check" />{/if}</span>
        </button>
      </li>
    </ul>
  {/if}
</div>

<style>
  .podcasts-menu {
    display: inline-flex;
  }

  .podcasts-menu__btn {
    display: inline-grid;
    place-items: center;
    width: var(--rk-control-h, 2.5rem);
    height: var(--rk-control-h, 2.5rem);
    padding: 0;
    border-radius: var(--rk-radius-control, var(--rk-radius));
    border: 1px solid var(--rk-line);
    background: transparent;
    color: var(--rk-ink);
    cursor: pointer;
  }

  .podcasts-menu__btn:hover,
  .podcasts-menu__btn[aria-expanded="true"] {
    border-color: color-mix(in srgb, var(--rk-accent) 45%, var(--rk-line));
  }

  .podcasts-menu__btn :global(.ui-ic) {
    width: 1.1rem;
    height: 1.1rem;
  }

  @media (min-width: 720px) {
    .podcasts-menu__refresh {
      display: none;
    }
  }

  .podcasts-menu__check {
    display: inline-grid;
    width: 1rem;
    margin-left: auto;
    color: var(--rk-accent);
  }

  .podcasts-menu__check :global(.ui-ic) {
    width: 1rem;
    height: 1rem;
  }
</style>
