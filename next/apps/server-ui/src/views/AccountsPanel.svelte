<script lang="ts">
  import { ActionRow, Button, EmptyState, Field, Panel, TextInput } from "@rekord/ui";
  import { admin } from "../lib/admin.svelte";
  import { t } from "../lib/i18n.svelte";

  let renaming = $state<string | null>(null);
  let renameValue = $state("");

  const accounts = $derived(admin.accounts);

  function startRename(id: string, name: string) {
    renaming = id;
    renameValue = name;
  }

  async function commitRename(id: string) {
    const name = renameValue.trim();
    renaming = null;
    if (name) await admin.renameAccount(id, name);
  }

  function confirmDelete(id: string, name: string) {
    if (!window.confirm(t("accounts.deleteConfirm", { name }))) return;
    void admin.deleteAccount(id);
  }
</script>

<Panel title={t("accounts.title")}>
  {#snippet actions()}
    <Button
      variant="secondary"
      disabled={admin.busy}
      onclick={() => void admin.loadSection("accounts")}
    >
      {t("common.refresh")}
    </Button>
  {/snippet}

  {#if accounts.length === 0}
    <EmptyState message={t("accounts.empty")} />
  {:else}
    <ul class="accounts">
      {#each accounts as acc (acc.id)}
        <li>
          {#if renaming === acc.id}
            <TextInput
              bind:value={renameValue}
              aria-label={t("accounts.renameAria", { name: acc.name })}
              onkeydown={(e) => {
                if ((e as KeyboardEvent).key === "Enter") void commitRename(acc.id);
                if ((e as KeyboardEvent).key === "Escape") renaming = null;
              }}
            />
            <Button disabled={admin.busy} onclick={() => void commitRename(acc.id)}>
              {t("common.save")}
            </Button>
            <Button variant="ghost" onclick={() => (renaming = null)}>{t("common.cancel")}</Button>
          {:else}
            <span class="name">
              {acc.name}
              {#if acc.id === admin.defaultAccountId}
                <span class="tag">{t("accounts.default")}</span>
              {/if}
            </span>
            <span class="id">{acc.id}</span>
            <Button
              variant="ghost"
              disabled={admin.busy}
              onclick={() => void admin.exportAccount(acc.id)}
            >
              {t("accounts.export")}
            </Button>
            <Button
              variant="ghost"
              disabled={admin.busy}
              onclick={() => startRename(acc.id, acc.name)}
            >
              {t("accounts.rename")}
            </Button>
            {#if acc.id !== admin.defaultAccountId}
              <Button
                variant="ghost"
                disabled={admin.busy}
                onclick={() => confirmDelete(acc.id, acc.name)}
              >
                {t("accounts.delete")}
              </Button>
            {/if}
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</Panel>

<Panel title={t("accounts.new.title")}>
  <Field label={t("accounts.new.name")}>
    <TextInput bind:value={admin.newAccountName} placeholder={t("accounts.new.placeholder")} />
  </Field>
  <ActionRow>
    <Button
      disabled={admin.busy || !admin.newAccountName.trim()}
      onclick={() => void admin.createAccount()}
    >
      {t("accounts.new.create")}
    </Button>
  </ActionRow>
  <p class="hint">{t("accounts.new.hint")}</p>
</Panel>

<style>
  .accounts {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .accounts li {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
    border: 1px solid var(--rk-line);
    border-radius: var(--rk-radius);
    background: var(--rk-surface-3);
    padding: 0.5rem 0.65rem;
    min-width: 0;
  }

  .name {
    font-weight: 650;
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .tag {
    font-size: var(--rk-fs-2xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--rk-accent);
  }

  .id {
    flex: 1 1 6rem;
    color: var(--rk-muted);
    font-size: var(--rk-fs-xs);
    overflow-wrap: anywhere;
  }

  .hint {
    margin: 0.7rem 0 0;
    color: var(--rk-muted);
    font-size: var(--rk-fs-sm);
    line-height: var(--rk-lh);
  }
</style>
