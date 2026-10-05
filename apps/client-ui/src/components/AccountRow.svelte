<script lang="ts">
  import { Button } from "@rekord/ui";

  let {
    name,
    selected = false,
    current = false,
    currentLabel = "",
    busy = false,
    level = null as number | null,
    levelTitle = "",
    levelLabel = "",
    defaultBadge = "",
    removeLabel,
    removeDisabled = false,
    removeTitle = undefined as string | undefined,
    renameLabel = "",
    renameSaveLabel = "",
    renameCancelLabel = "",
    renamePlaceholder = "",
    onselect,
    onremove,
    onrename,
  }: {
    name: string;
    selected?: boolean;
    /** The account this client is bound to right now. */
    current?: boolean;
    /** Badge text for the current account ("In uso"). */
    currentLabel?: string;
    busy?: boolean;
    level?: number | null;
    levelTitle?: string;
    levelLabel?: string;
    defaultBadge?: string;
    removeLabel: string;
    removeDisabled?: boolean;
    removeTitle?: string;
    renameLabel?: string;
    renameSaveLabel?: string;
    renameCancelLabel?: string;
    renamePlaceholder?: string;
    onselect: () => void;
    onremove: () => void;
    /** Enables inline rename; resolves when the hub answered. */
    onrename?: (name: string) => Promise<void> | void;
  } = $props();

  const initial = $derived((name.trim()[0] || "?").toUpperCase());

  let editing = $state(false);
  let draft = $state("");
  let inputEl: HTMLInputElement | null = $state(null);

  function startRename() {
    draft = name;
    editing = true;
    queueMicrotask(() => {
      inputEl?.focus();
      inputEl?.select();
    });
  }

  async function saveRename() {
    const next = draft.trim();
    if (!next || next === name) {
      editing = false;
      return;
    }
    await onrename?.(next);
    editing = false;
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      void saveRename();
    } else if (e.key === "Escape") {
      e.preventDefault();
      editing = false;
    }
  }
</script>

<div class="account-row" class:is-selected={selected} class:is-current={current} role="listitem">
  {#if editing}
    <div class="account-row__edit">
      <span class="account-row__avatar" aria-hidden="true">{initial}</span>
      <input
        bind:this={inputEl}
        class="account-row__input"
        bind:value={draft}
        placeholder={renamePlaceholder}
        aria-label={renameLabel}
        maxlength="64"
        disabled={busy}
        onkeydown={onKey}
      />
    </div>
    <div class="account-row__actions">
      <Button disabled={busy || !draft.trim()} onclick={() => void saveRename()}>
        {renameSaveLabel}
      </Button>
      <Button variant="ghost" disabled={busy} onclick={() => (editing = false)}>
        {renameCancelLabel}
      </Button>
    </div>
  {:else}
    <button
      type="button"
      class="account-row__main"
      disabled={busy || selected}
      aria-current={current ? "true" : undefined}
      onclick={onselect}
    >
      <span class="account-row__avatar" aria-hidden="true">{initial}</span>
      <span class="account-row__text">
        <span class="account-row__name">{name}</span>
        <span class="account-row__badges">
          {#if current && currentLabel}
            <span class="account-row__current">{currentLabel}</span>
          {/if}
          {#if level != null}
            <span class="account-row__level-pill" title={levelTitle || undefined}>
              {levelLabel}
            </span>
          {:else if defaultBadge}
            <span class="account-row__badge">{defaultBadge}</span>
          {/if}
        </span>
      </span>
    </button>
    <div class="account-row__actions">
      {#if onrename && renameLabel}
        <Button variant="ghost" disabled={busy} onclick={startRename}>{renameLabel}</Button>
      {/if}
      <Button
        variant="ghost"
        disabled={busy || removeDisabled}
        title={removeTitle}
        onclick={onremove}
      >
        {removeLabel}
      </Button>
    </div>
  {/if}
</div>

<style>
  .account-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 0.7rem;
    align-items: center;
    padding: 0.75rem 0.85rem;
    border: 1px solid var(--rk-line);
    border-radius: var(--rk-radius-lg);
    background: color-mix(in srgb, var(--rk-surface-2) 78%, transparent);
  }

  .account-row__actions {
    display: flex;
    gap: 0.35rem;
    align-items: center;
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  .account-row__badges {
    display: inline-flex;
    gap: 0.35rem;
    align-items: center;
    flex-wrap: wrap;
  }

  .account-row__current {
    font-size: var(--rk-fs-3xs);
    font-weight: 750;
    letter-spacing: 0.04em;
    line-height: 1;
    padding: 0.32em 0.62em;
    border-radius: var(--rk-radius-round);
    background: var(--rk-accent);
    color: var(--rk-on-accent, #fff);
    white-space: nowrap;
  }

  .account-row__edit {
    min-width: 0;
    display: grid;
    grid-template-columns: 2.25rem minmax(0, 1fr);
    gap: 0.75rem;
    align-items: center;
  }

  .account-row__input {
    min-width: 0;
    width: 100%;
    font: inherit;
    font-weight: 700;
    color: var(--rk-ink);
    background: var(--rk-surface-3);
    border: 1px solid var(--rk-line);
    border-radius: var(--rk-radius);
    padding: 0.45rem 0.6rem;
  }

  .account-row__input:focus-visible {
    outline: 2px solid var(--rk-accent);
    outline-offset: 1px;
  }

  .account-row.is-current {
    border-color: color-mix(in srgb, var(--rk-accent) 55%, var(--rk-line) 45%);
  }

  .account-row.is-selected {
    border-color: color-mix(in srgb, var(--rk-accent) 42%, var(--rk-line) 58%);
    background: color-mix(in srgb, var(--rk-accent) 11%, var(--rk-surface-2) 89%);
  }

  .account-row__main {
    min-width: 0;
    display: grid;
    grid-template-columns: 2.25rem minmax(0, 1fr);
    gap: 0.75rem;
    align-items: center;
    text-align: left;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
    padding: 0;
  }

  .account-row__main:disabled {
    cursor: default;
  }

  .account-row__avatar {
    display: grid;
    width: 2.25rem;
    height: 2.25rem;
    place-items: center;
    border-radius: var(--rk-radius-round);
    background: color-mix(in srgb, var(--rk-accent) 18%, var(--rk-surface-3) 82%);
    color: var(--rk-accent);
    font-weight: 850;
  }

  .account-row__text {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: 0.55rem;
    min-width: 0;
  }

  .account-row__name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--rk-fs-md);
    font-weight: 700;
    letter-spacing: -0.015em;
    line-height: var(--rk-lh-snug);
  }

  .account-row__badge,
  .account-row__level-pill {
    flex: 0 0 auto;
    font-size: var(--rk-fs-3xs);
    font-weight: 750;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    line-height: 1;
    padding: 0.32em 0.62em;
    border-radius: var(--rk-radius-round);
    border: 1px solid color-mix(in srgb, var(--rk-accent) 30%, var(--rk-line) 70%);
    background: color-mix(in srgb, var(--rk-accent) 8%, var(--rk-surface-3) 92%);
    color: color-mix(in srgb, var(--rk-accent) 80%, var(--rk-muted) 20%);
    white-space: nowrap;
  }

  .account-row.is-selected .account-row__name {
    color: color-mix(in srgb, var(--rk-accent) 18%, var(--rk-ink) 82%);
  }

  .account-row.is-selected .account-row__level-pill {
    border-color: color-mix(in srgb, var(--rk-accent) 44%, var(--rk-line) 56%);
    background: color-mix(in srgb, var(--rk-accent) 14%, var(--rk-surface-3) 86%);
    color: color-mix(in srgb, var(--rk-accent) 92%, var(--rk-ink) 8%);
  }

  /* Telefono: la pillola del livello scende sotto il nome. Accanto ad esso, in
     150px che restano fra avatar e bottone, mangiava metà riga e i nomi finivano
     tutti in «TestAcc…» — e il nome è l'unica cosa che distingue una riga. */
  @media (max-width: 559.98px) {
    .account-row__text {
      grid-template-columns: minmax(0, 1fr);
      justify-items: start;
      gap: 0.2rem;
    }
  }
</style>
