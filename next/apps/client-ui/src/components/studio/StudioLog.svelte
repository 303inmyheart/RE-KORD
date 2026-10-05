<script lang="ts" module>
  export type StudioLogKind = "info" | "ok" | "warn" | "error" | "detail";
  export type StudioLogEntry = { id: number; kind: StudioLogKind; text: string };
</script>

<script lang="ts">
  import { tick } from "svelte";
  import { t } from "../../lib/i18n.svelte";

  let {
    entries,
    raw = "",
    showRaw = $bindable(false),
    title,
    onclear,
    class: className = "",
  }: {
    entries: StudioLogEntry[];
    /** Unfiltered tool output, shown behind a toggle when present. */
    raw?: string;
    showRaw?: boolean;
    title?: string;
    onclear?: () => void;
    class?: string;
  } = $props();

  let body: HTMLElement | null = $state(null);
  let stick = true;

  function onScroll() {
    const el = body;
    if (!el) return;
    stick = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
  }

  // Follow new lines unless the user scrolled up to read.
  $effect(() => {
    entries.length;
    showRaw;
    raw;
    if (!stick) return;
    void tick().then(() => {
      if (body) body.scrollTop = body.scrollHeight;
    });
  });

  const empty = $derived(showRaw ? !raw.trim() : entries.length === 0);
</script>

<section class="studio-log-panel {className}" aria-label={title ?? t("studio.log.title")}>
  <header class="studio-log-panel__head">
    <h4 class="studio-panel-title studio-log-panel__title">{title ?? t("studio.log.title")}</h4>
    <div class="studio-log-panel__tools">
      {#if raw.trim()}
        <label class="studio-log-panel__raw">
          <input type="checkbox" bind:checked={showRaw} />
          <span>{t("studio.log.showRaw")}</span>
        </label>
      {/if}
      {#if onclear}
        <button
          type="button"
          class="linkbtn studio-log-panel__clear"
          disabled={entries.length === 0 && !raw}
          onclick={() => onclear?.()}
        >
          {t("studio.log.clear")}
        </button>
      {/if}
    </div>
  </header>
  <div
    class="studio-log-panel__body rk-scroll"
    class:is-raw={showRaw}
    bind:this={body}
    onscroll={onScroll}
    role="log"
    aria-live="polite"
    tabindex="-1"
  >
    {#if empty}
      <p class="studio-log-panel__empty">{t("studio.log.empty")}</p>
    {:else if showRaw}
      <pre class="studio-log-panel__pre">{raw}</pre>
    {:else}
      <ol class="studio-log-panel__list">
        {#each entries as e (e.id)}
          <li class="studio-log-line studio-log-line--{e.kind}">{e.text}</li>
        {/each}
      </ol>
    {/if}
  </div>
</section>
