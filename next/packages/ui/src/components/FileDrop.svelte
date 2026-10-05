<script lang="ts">
  /**
   * File picker that also accepts drag & drop. Replaces the bare
   * `<input type="file">` (which renders the browser's own, untranslated button).
   *
   * ```svelte
   * <FileDrop accept="image/*" label="Trascina un'immagine o scegli un file"
   *   hint="JPG o PNG, almeno 500×500" fileName={picked?.name}
   *   onfiles={(files) => (picked = files[0])} />
   * ```
   */
  let {
    accept = "",
    multiple = false,
    disabled = false,
    label,
    hint = "",
    /** Name of the file already chosen (shown instead of the label). */
    fileName = "",
    /** Text of the visible "choose" affordance. */
    buttonLabel = "",
    /** One-line layout for dialogs with little room. */
    compact = false,
    id = "",
    onfiles,
    icon,
    class: className = "",
  }: {
    accept?: string;
    multiple?: boolean;
    disabled?: boolean;
    label: string;
    hint?: string;
    fileName?: string;
    buttonLabel?: string;
    compact?: boolean;
    id?: string;
    onfiles: (files: File[]) => void;
    icon?: import("svelte").Snippet;
    class?: string;
  } = $props();

  let input: HTMLInputElement | null = $state(null);
  let over = $state(false);
  /** dragenter/leave fire for every child: count them instead of toggling. */
  let depth = 0;

  function accepts(file: File): boolean {
    if (!accept.trim()) return true;
    const name = file.name.toLowerCase();
    const type = (file.type || "").toLowerCase();
    return accept
      .split(",")
      .map((s) => s.trim().toLowerCase())
      .filter(Boolean)
      .some((rule) => {
        if (rule.startsWith(".")) return name.endsWith(rule);
        if (rule.endsWith("/*")) return type.startsWith(rule.slice(0, -1));
        return type === rule;
      });
  }

  function deliver(list: FileList | null | undefined) {
    if (!list || disabled) return;
    const files = [...list].filter(accepts);
    if (files.length === 0) return;
    onfiles(multiple ? files : files.slice(0, 1));
  }

  function onDragEnter(e: DragEvent) {
    if (disabled || !e.dataTransfer?.types.includes("Files")) return;
    e.preventDefault();
    depth += 1;
    over = true;
  }

  function onDragOver(e: DragEvent) {
    if (disabled || !e.dataTransfer?.types.includes("Files")) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
  }

  function onDragLeave() {
    depth = Math.max(0, depth - 1);
    if (depth === 0) over = false;
  }

  function onDrop(e: DragEvent) {
    e.preventDefault();
    depth = 0;
    over = false;
    deliver(e.dataTransfer?.files);
  }
</script>

<div
  class="rk-filedrop {className}"
  class:is-over={over}
  class:is-disabled={disabled}
  class:is-compact={compact}
  class:has-file={!!fileName}
  role="presentation"
  ondragenter={onDragEnter}
  ondragover={onDragOver}
  ondragleave={onDragLeave}
  ondrop={onDrop}
>
  <input
    bind:this={input}
    {id}
    class="rk-visually-hidden"
    type="file"
    tabindex="-1"
    {accept}
    {multiple}
    {disabled}
    onchange={(e) => {
      deliver((e.currentTarget as HTMLInputElement).files);
      (e.currentTarget as HTMLInputElement).value = "";
    }}
  />
  <button
    type="button"
    class="rk-filedrop__hit"
    {disabled}
    aria-describedby={hint ? `${id || "rk-filedrop"}-hint` : undefined}
    onclick={() => input?.click()}
  >
    <span class="rk-filedrop__ic" aria-hidden="true">
      {#if icon}
        {@render icon()}
      {:else}
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M12 16V4" />
          <path d="m7 9 5-5 5 5" />
          <path d="M4 16v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2" />
        </svg>
      {/if}
    </span>
    <span class="rk-filedrop__text">
      <span class="rk-filedrop__label">{fileName || label}</span>
      {#if hint && !compact}
        <span class="rk-filedrop__hint" id={`${id || "rk-filedrop"}-hint`}>{hint}</span>
      {/if}
    </span>
    {#if buttonLabel}
      <span class="rk-filedrop__btn">{buttonLabel}</span>
    {/if}
  </button>
</div>

<style>
  .rk-filedrop {
    position: relative;
    min-width: 0;
  }

  .rk-filedrop__hit {
    display: flex;
    align-items: center;
    gap: var(--rk-space-lg);
    width: 100%;
    min-height: 4.5rem;
    padding: var(--rk-space-lg) var(--rk-space-xl);
    border: 1.5px dashed color-mix(in srgb, var(--rk-muted) 45%, var(--rk-line));
    border-radius: var(--rk-radius-card);
    background: color-mix(in srgb, var(--rk-surface-3) 40%, transparent);
    color: var(--rk-ink);
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition:
      border-color 0.15s ease,
      background 0.15s ease;
  }

  .is-compact .rk-filedrop__hit {
    min-height: var(--rk-control-h);
    padding: var(--rk-space-xs) var(--rk-space-lg);
    gap: var(--rk-space-md);
  }

  .rk-filedrop__hit:hover:not(:disabled),
  .is-over .rk-filedrop__hit {
    border-color: var(--rk-accent-2);
    background: var(--rk-accent2-soft);
  }

  .has-file .rk-filedrop__hit {
    border-style: solid;
    border-color: color-mix(in srgb, var(--rk-accent-2) 45%, var(--rk-line));
  }

  .rk-filedrop__hit:disabled {
    cursor: not-allowed;
    color: var(--rk-muted);
    background: transparent;
  }

  .rk-filedrop__ic {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 2.25rem;
    height: 2.25rem;
    border-radius: var(--rk-radius-round);
    background: var(--rk-accent2-soft);
    color: var(--rk-accent-2);
  }

  .is-compact .rk-filedrop__ic {
    width: 1.75rem;
    height: 1.75rem;
  }

  .rk-filedrop__ic :global(svg) {
    width: 1.15rem;
    height: 1.15rem;
  }

  .rk-filedrop__text {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 0.15rem;
  }

  .rk-filedrop__label {
    font-size: var(--rk-fs-3);
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  .rk-filedrop__hint {
    font-size: var(--rk-fs-2);
    color: var(--rk-muted);
  }

  .rk-filedrop__btn {
    flex-shrink: 0;
    padding: 0.3rem 0.7rem;
    border: 1px solid var(--rk-line-strong);
    border-radius: var(--rk-radius-control);
    font-size: var(--rk-fs-2);
    font-weight: 650;
    color: var(--rk-ink);
  }
</style>
