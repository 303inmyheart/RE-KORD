<script lang="ts">
  import type { Snippet } from "svelte";
  import { sheetDrag, SHEET_MEDIA_QUERY } from "../lib/sheetDrag";
  import { modalSurface } from "../lib/modalStack";
  import { uiLabels } from "../lib/uiLabels.svelte";

  let {
    open = false,
    title = "",
    eyebrow = "",
    lede = "",
    panelClass = "",
    closeLabel,
    history = true,
    onclose,
    lead,
    children,
    footer,
  }: {
    open?: boolean;
    title?: string;
    eyebrow?: string;
    /** Optional path / subtitle under the title (meta-edit parity). */
    lede?: string;
    /** Extra class on the dialog panel (e.g. wider reading layouts). */
    panelClass?: string;
    /** Accessible name of the × button; defaults to `setUiLabels({ close })`. */
    closeLabel?: string;
    /** Push a history entry so Android Back closes the dialog first. */
    history?: boolean;
    onclose: () => void;
    /** Optional media / avatar left of the title block (entity-info parity). */
    lead?: Snippet;
    children: Snippet;
    footer?: Snippet;
  } = $props();

  const uid = $props.id();
  const titleId = `${uid}-title`;
  const ledeId = `${uid}-lede`;

  /* Su telefono il dialogo è un foglio dal basso e si può spingere giù per
     chiuderlo; su schermo grande resta un pannello centrato. */
  let isSheet = $state(
    typeof window !== "undefined" && window.matchMedia(SHEET_MEDIA_QUERY).matches,
  );

  $effect(() => {
    const mq = window.matchMedia(SHEET_MEDIA_QUERY);
    const sync = () => {
      isSheet = mq.matches;
    };
    sync();
    mq.addEventListener("change", sync);
    return () => mq.removeEventListener("change", sync);
  });

  /** Escape stacking contexts (glass panels / isolation) — same as CustomThemeDialog. */
  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return {
      destroy() {
        if (node.parentNode) node.parentNode.removeChild(node);
      },
    };
  }
</script>

{#if open}
  <div
    class="rk-modal-back rk-sheet-back"
    role="presentation"
    use:portal
    onclick={(e) => {
      if (e.target === e.currentTarget) onclose();
    }}
  >
    <!-- Escape, focus trap / restore, scroll lock and Back: modalSurface. -->
    <div
      class={["rk-modal", "rk-sheet", "rk-scroll", panelClass]
        .filter(Boolean)
        .join(" ")}
      role="dialog"
      aria-modal="true"
      aria-labelledby={titleId}
      aria-describedby={lede ? ledeId : undefined}
      tabindex="-1"
      use:modalSurface={{ onclose, focusPanelOnly: isSheet, history }}
      use:sheetDrag={{
        enabled: isSheet,
        gripSelector: "[data-sheet-grip]",
        onclose,
      }}
    >
      <div class="rk-sheet__grip" data-sheet-grip aria-hidden="true"></div>
      <header class="head" data-sheet-grip>
        <div class="head-main">
          {#if lead}
            <div class="lead">{@render lead()}</div>
          {/if}
          <div class="titles">
            {#if eyebrow}
              <p class="eyebrow">{eyebrow}</p>
            {/if}
            <h2 id={titleId}>{title}</h2>
            {#if lede}
              <p class="lede" id={ledeId}>{lede}</p>
            {/if}
          </div>
        </div>
        <button
          type="button"
          class="close"
          data-modal-close
          onclick={onclose}
          aria-label={closeLabel ?? uiLabels.close}
          title={closeLabel ?? uiLabels.close}
        >
          <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
            <path
              fill="currentColor"
              d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"
            />
          </svg>
        </button>
      </header>
      <div class="body" data-sheet-body>{@render children()}</div>
      {#if footer}
        <footer class="foot" data-sheet-foot>{@render footer()}</footer>
      {/if}
    </div>
  </div>
{/if}

<style>
  /* Zero specificity on the shape rules: on a phone `styles/sheet.css` reshapes
     this panel into a bottom sheet with a plain class, and it must win. */
  :where(.rk-modal-back) {
    position: fixed;
    inset: 0;
    z-index: var(--rk-z-modal, 120);
    background: rgba(4, 10, 18, 0.72);
    display: grid;
    place-items: center;
    /* Notch, status bar and home indicator all stay clear of the dialog. */
    padding: max(1rem, env(safe-area-inset-top, 0px))
      max(1rem, env(safe-area-inset-right, 0px))
      max(1rem, env(safe-area-inset-bottom, 0px))
      max(1rem, env(safe-area-inset-left, 0px));
  }

  :where(.rk-modal) {
    width: min(28rem, 100%);
    /* --rk-app-vh, not dvh: the on-screen keyboard overlays the page instead of
       shrinking it, and a dialog with a field in it has to stay above the keys. */
    max-height: min(calc(var(--rk-app-vh) * 0.9), 900px);
    overflow: auto;
    overscroll-behavior: contain;
    background: var(--rk-surface);
    border: 1px solid var(--rk-line);
    border-radius: var(--rk-radius-sheet);
    box-shadow: var(--rk-shadow);
    outline: none;
  }

  .head {
    display: flex;
    justify-content: space-between;
    gap: 0.75rem;
    align-items: flex-start;
    padding: 0.7rem 0.85rem 0.55rem;
    border-bottom: 1px solid var(--rk-line);
  }

  .head-main {
    display: flex;
    align-items: center;
    gap: 0.85rem;
    min-width: 0;
    flex: 1 1 auto;
  }

  .lead {
    flex: 0 0 auto;
    display: grid;
    place-items: center;
  }

  .titles {
    min-width: 0;
  }

  .eyebrow {
    margin: 0;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    font-size: var(--rk-fs-eyebrow);
    color: var(--rk-muted);
    font-weight: 650;
  }

  h2 {
    margin: 0.15rem 0 0;
    font-size: var(--rk-fs-base);
    line-height: var(--rk-lh-snug);
  }

  /* Often a file path: UI font, wrapped at spaces and dashes first and only
     split inside a long token when it really does not fit. */
  .lede {
    margin: 0.3rem 0 0;
    font-size: var(--rk-fs-2);
    line-height: var(--rk-lh-snug);
    color: var(--rk-muted);
    overflow-wrap: anywhere;
    word-break: normal;
  }

  .close {
    flex: 0 0 auto;
    width: 2rem;
    height: 2rem;
    display: inline-grid;
    place-items: center;
    border: 0;
    background: transparent;
    color: var(--rk-muted);
    cursor: pointer;
    padding: 0;
    border-radius: var(--rk-radius-sm);
  }

  .close:hover {
    color: var(--rk-ink);
    background: var(--rk-surface-3);
  }

  .body {
    padding: 0.7rem 0.85rem;
    display: grid;
    gap: 0.55rem;
  }

  /* Fields già usano margin: annulla lo stacking con il gap del body. */
  .body :global(.rk-field) {
    margin-bottom: 0;
    gap: 0.28rem;
    font-size: var(--rk-fs-sm);
  }

  .body :global(.rk-input) {
    padding: 0.42rem 0.65rem;
  }

  .foot {
    padding: 0.55rem 0.85rem 0.7rem;
    border-top: 1px solid var(--rk-line);
    display: flex;
    gap: 0.45rem;
    justify-content: flex-end;
    flex-wrap: wrap;
    align-items: center;
  }

  .foot :global(.rk-modal-foot-spacer) {
    flex: 1 1 0.5rem;
    min-width: 0.5rem;
  }
</style>
