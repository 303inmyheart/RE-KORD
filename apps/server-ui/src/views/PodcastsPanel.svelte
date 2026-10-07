<script lang="ts">
  import { onMount } from "svelte";
  import { ActionRow, Banner, Button, EmptyState, Field, Panel, TextInput } from "@rekord/ui";
  import type { PodcastSourceAdmin } from "../api";
  import { admin } from "../lib/admin.svelte";
  import { formatDateTime, t } from "../lib/i18n.svelte";
  import { podcastsAdmin as pa } from "../lib/podcastsAdmin.svelte";
  import { hubErrorKey } from "../lib/hubErrors";

  onMount(() => {
    void pa.load();
  });

  const data = $derived(pa.data);
  const locked = $derived(pa.busy || !admin.canManage);
  const limits = $derived(data?.limits);
  const counts = $derived(
    Array.from({ length: limits?.maxEpisodes ?? 20 }, (_, i) => i + 1),
  );

  let editing = $state<number | null>(null);
  let editName = $state("");

  function startEdit(src: PodcastSourceAdmin) {
    editing = src.id;
    editName = src.nameCustom ? src.name : "";
  }

  async function commitEdit(src: PodcastSourceAdmin) {
    editing = null;
    await pa.rename(src.id, editName.trim());
  }

  function confirmRemove(src: PodcastSourceAdmin) {
    if (!window.confirm(t("podcasts.removeConfirm", { name: src.name }))) return;
    void pa.remove(src.id, src.name);
  }

  /** Error code of a source in the panel's language. */
  function codeText(code: string): string {
    const k = hubErrorKey(502, code, true);
    return k ? t(k.key, k.vars) : code;
  }

  function fmtLength(secs?: number | null): string {
    if (!secs || secs <= 0) return "";
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = String(Math.floor(secs % 60)).padStart(2, "0");
    return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
  }
</script>

<Panel title={t("podcasts.module.title")}>
  {#snippet actions()}
    <Button variant="secondary" disabled={pa.busy} onclick={() => void pa.load()}>
      {t("common.refresh")}
    </Button>
  {/snippet}
  {#if data}
    <div class="state">
      <span class="k">{t("podcasts.module.state")}</span>
      <span class="v" class:on={data.enabled}>
        {data.enabled ? t("podcasts.module.on") : t("podcasts.module.off")}
      </span>
      <span class="k">{t("podcasts.module.sources")}</span>
      <span class="v">{data.sources.length} / {data.limits.maxSources}</span>
    </div>
    <ActionRow>
      <Button disabled={locked} onclick={() => void pa.setEnabled(!data.enabled)}>
        {data.enabled ? t("podcasts.module.disable") : t("podcasts.module.enable")}
      </Button>
    </ActionRow>
    <p class="hint">{t("podcasts.module.hint")}</p>

    <div class="ttl">
      <Field label={t("podcasts.ttl.label")}>
        <input
          class="rk-input num"
          type="number"
          min={data.limits.minTtlMinutes}
          max={data.limits.maxTtlMinutes}
          step="5"
          bind:value={pa.ttl}
          disabled={locked}
        />
      </Field>
      <Button
        variant="secondary"
        disabled={locked || pa.ttl === data.cacheTtlMinutes}
        onclick={() => void pa.saveTtl()}
      >
        {t("common.save")}
      </Button>
    </div>
    <p class="hint">{t("podcasts.ttl.hint")}</p>
  {/if}
</Panel>

<Panel title={t("podcasts.add.title")}>
  <Field label={t("podcasts.add.url")}>
    <TextInput
      bind:value={pa.url}
      placeholder="https://…"
      disabled={locked}
      oninput={() => {
        pa.preview = null;
        pa.formError = "";
      }}
    />
  </Field>
  <div class="row2">
    <Field label={t("podcasts.add.name")}>
      <TextInput bind:value={pa.name} placeholder={t("podcasts.add.namePlaceholder")} disabled={locked} />
    </Field>
    <Field label={t("podcasts.add.count")}>
      <select class="rk-input num" bind:value={pa.count} disabled={locked}>
        {#each counts as n (n)}
          <option value={n}>{n}</option>
        {/each}
      </select>
    </Field>
  </div>
  <ActionRow>
    <Button variant="secondary" disabled={locked || pa.testing || !pa.url.trim()} onclick={() => void pa.test()}>
      {pa.testing ? t("podcasts.add.testing") : t("podcasts.add.test")}
    </Button>
    <Button disabled={locked || pa.testing || !pa.url.trim()} onclick={() => void pa.add()}>
      {t("podcasts.add.save")}
    </Button>
  </ActionRow>

  {#if pa.formError}
    <Banner tone="error">{pa.formError}</Banner>
  {/if}
  {#if pa.preview}
    <div class="preview">
      <p class="preview-head">
        <span class="kind">{t(`podcasts.kind.${pa.preview.kind}`)}</span>
        {#if pa.preview.title}<strong>{pa.preview.title}</strong>{/if}
      </p>
      {#if pa.preview.live}
        <p class="hint">{t("podcasts.add.liveOk")}</p>
      {:else}
        <ol class="eps">
          {#each pa.preview.episodes as ep (ep.key)}
            <li>
              <span class="ep-title">{ep.title}</span>
              <span class="ep-meta">
                {formatDateTime(ep.publishedAt)}{#if fmtLength(ep.durationSecs)}{" · "}{fmtLength(ep.durationSecs)}{/if}
              </span>
            </li>
          {/each}
        </ol>
      {/if}
    </div>
  {/if}
  <p class="hint">{t("podcasts.add.hint")}</p>
  {#if data && !data.ytdlpEnabled}
    <p class="hint">{t("podcasts.add.noYtdlp")}</p>
  {/if}
</Panel>

<Panel title={t("podcasts.list.title")}>
  {#if !data || data.sources.length === 0}
    <EmptyState message={t("podcasts.list.empty")} />
  {:else}
    <ul class="sources">
      {#each data.sources as src, i (src.id)}
        <li>
          <div class="src-main">
            {#if editing === src.id}
              <div class="edit">
                <TextInput
                  bind:value={editName}
                  placeholder={t("podcasts.list.namePlaceholder")}
                  aria-label={t("podcasts.list.rename")}
                  onkeydown={(e) => {
                    if ((e as KeyboardEvent).key === "Enter") void commitEdit(src);
                    if ((e as KeyboardEvent).key === "Escape") editing = null;
                  }}
                />
                <Button disabled={locked} onclick={() => void commitEdit(src)}>{t("common.save")}</Button>
                <Button variant="ghost" onclick={() => (editing = null)}>{t("common.cancel")}</Button>
              </div>
            {:else}
              <span class="name">
                {src.name}
                <span class="tag">{t(`podcasts.kind.${src.kind}`)}</span>
              </span>
            {/if}
            <span class="url">{src.url}</span>
            <span class="meta">
              {#if src.live}
                {t("podcasts.list.live")}
              {:else}
                {t("podcasts.list.fetched", { when: formatDateTime(src.fetchedAt) })}
              {/if}
            </span>
            {#if src.error}
              <span class="err">{codeText(src.error)}</span>
            {/if}
          </div>
          <div class="src-tools">
            {#if !src.live}
              <label class="count">
                <span>{t("podcasts.list.episodes")}</span>
                <select
                  class="rk-input num"
                  value={src.episodeCount}
                  disabled={locked}
                  onchange={(e) => void pa.setCount(src.id, Number((e.currentTarget as HTMLSelectElement).value))}
                >
                  {#each counts as n (n)}
                    <option value={n}>{n}</option>
                  {/each}
                </select>
              </label>
            {/if}
            <Button variant="ghost" disabled={locked || i === 0} onclick={() => void pa.move(src.id, -1)}>
              {t("podcasts.list.up")}
            </Button>
            <Button
              variant="ghost"
              disabled={locked || i === data.sources.length - 1}
              onclick={() => void pa.move(src.id, 1)}
            >
              {t("podcasts.list.down")}
            </Button>
            <Button variant="ghost" disabled={locked} onclick={() => startEdit(src)}>
              {t("podcasts.list.rename")}
            </Button>
            <Button variant="ghost" disabled={locked} onclick={() => confirmRemove(src)}>
              {t("common.remove")}
            </Button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</Panel>

{#if !admin.canManage}
  <Banner tone="info">{t("integrations.machineOnly")}</Banner>
{/if}

<style>
  .state {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.3rem 0.8rem;
    align-items: baseline;
    margin-bottom: 0.8rem;
  }

  .k {
    font-size: var(--rk-fs-xs);
    color: var(--rk-muted);
  }

  .v {
    font-weight: 600;
  }

  .v.on {
    color: var(--rk-success, #2fa66a);
  }

  .hint {
    margin: 0.7rem 0 0;
    color: var(--rk-muted);
    font-size: var(--rk-fs-sm);
    line-height: var(--rk-lh);
  }

  .ttl {
    display: flex;
    align-items: flex-end;
    gap: 0.6rem;
    flex-wrap: wrap;
    margin-top: 1rem;
  }

  .row2 {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 7rem;
    gap: 0.6rem;
  }

  .num {
    min-width: 5rem;
    width: auto;
  }

  .preview {
    margin-top: 0.8rem;
    border: 1px solid var(--rk-line);
    border-radius: var(--rk-radius);
    background: var(--rk-surface-3);
    padding: 0.65rem 0.8rem;
  }

  .preview-head {
    margin: 0 0 0.4rem;
    display: flex;
    gap: 0.5rem;
    align-items: baseline;
    flex-wrap: wrap;
  }

  .kind,
  .tag {
    font-size: var(--rk-fs-2xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--rk-accent);
  }

  .eps {
    margin: 0;
    padding-left: 1.2rem;
    display: grid;
    gap: 0.25rem;
  }

  .ep-title {
    font-weight: 600;
  }

  .ep-meta {
    display: block;
    color: var(--rk-muted);
    font-size: var(--rk-fs-xs);
  }

  .sources {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .sources li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    border: 1px solid var(--rk-line);
    border-radius: var(--rk-radius);
    background: var(--rk-surface-3);
    padding: 0.55rem 0.7rem;
  }

  .src-main {
    display: grid;
    gap: 0.15rem;
    min-width: 0;
    flex: 1 1 16rem;
  }

  .name {
    font-weight: 650;
    display: flex;
    gap: 0.45rem;
    align-items: baseline;
    flex-wrap: wrap;
  }

  .url,
  .meta {
    color: var(--rk-muted);
    font-size: var(--rk-fs-xs);
    overflow-wrap: anywhere;
  }

  .err {
    color: var(--rk-danger);
    font-size: var(--rk-fs-xs);
  }

  .edit {
    display: flex;
    gap: 0.4rem;
    flex-wrap: wrap;
    align-items: center;
  }

  .src-tools {
    display: flex;
    gap: 0.3rem;
    flex-wrap: wrap;
    align-items: center;
  }

  .count {
    display: inline-flex;
    gap: 0.35rem;
    align-items: center;
    font-size: var(--rk-fs-xs);
    color: var(--rk-muted);
  }

  .count .num {
    min-width: 3.5rem;
    padding: 0.3rem 0.4rem;
  }

  @media (max-width: 599.98px) {
    .row2 {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
