<script lang="ts">
  import { onMount } from "svelte";
  import { ActionRow, Button, Field, Panel, TextInput } from "@rekord/ui";
  import AccountList from "../AccountList.svelte";
  import AccountRow from "../AccountRow.svelte";
  import { getSelectedAccountId, type AccountsResponse } from "../../lib/account";
  import { accountAchievements } from "../../lib/accountLevel.svelte";
  import { buildAchievementsSnapshot, titleForNumericLevel } from "../../lib/achievements";
  import { api } from "../../lib/api";
  import { confirmDialog } from "../../lib/confirm.svelte";
  import { primaryGenre } from "../../lib/genres";
  import { t } from "../../lib/i18n.svelte";
  import { session } from "../../lib/session.svelte";
  import type { SettingsAccounts } from "./settingsAccounts.svelte";

  let { ctx }: { ctx: SettingsAccounts } = $props();

  let newAccountName = $state("");
  let accountBusy = $state(false);
  let accountErr = $state("");
  let accountOk = $state("");
  /** accountId → numeric level of the other accounts (legacy account-row pill). */
  let accountLevels = $state<Record<string, number>>({});
  /** The bound account's level: the same shared snapshot as the rail ring. */
  const currentLevel = $derived(accountAchievements.level);
  const currentId = $derived(session.activeAccountId || getSelectedAccountId());
  let okTimer: ReturnType<typeof setTimeout> | null = null;

  const shownErr = $derived(accountErr || ctx.loadError);
  /** Create / rename / delete are library operations: refused to remote clients without remote admin. */
  const canEdit = $derived(ctx.canManageLibrary);

  function flashOk(message: string, ms: number) {
    accountOk = message;
    if (okTimer) clearTimeout(okTimer);
    okTimer = setTimeout(() => {
      accountOk = "";
      okTimer = null;
    }, ms);
  }

  onMount(() => {
    void ctx.load().then((data) => {
      if (data) void loadAccountLevels(data);
    });
    return () => {
      if (okTimer) clearTimeout(okTimer);
    };
  });

  /** Level of another account, from its hub state (the current one is live). */
  async function levelForAccountId(accountId: string): Promise<number | null> {
    if (accountId === getSelectedAccountId()) return null;
    try {
      await session.ensureCatalogTracks();
      const tracks = session.catalogTracks;
      const libraryTrackCount = session.stats?.track_count ?? tracks.length;
      const [state, favorites, playlists] = await Promise.all([
        api.getUserStateForAccount(accountId),
        api.favoritesForAccount(accountId),
        api.playlistsForAccount(accountId),
      ]);
      const playlistTrackCount = playlists.reduce((s, p) => s + (p.track_count ?? 0), 0);
      return buildAchievementsSnapshot({
        playCounts: state.playCounts ?? {},
        tracks,
        favoritesCount: favorites.length,
        playlistsCount: playlists.length,
        playlistTrackCount,
        libraryTrackCount,
        shuffleBlocks:
          (state.excludedRelPaths?.length ?? 0) + (state.excludedAlbumIds?.length ?? 0),
        genreForTrack: (tr) => primaryGenre(tr),
      }).level.level;
    } catch {
      return null;
    }
  }

  function levelOf(accountId: string): number | null {
    if (accountId === currentId) return currentLevel;
    return accountLevels[accountId] ?? null;
  }

  async function renameAccount(id: string, name: string) {
    const next = name.trim();
    if (!next) return;
    accountBusy = true;
    accountErr = "";
    accountOk = "";
    try {
      ctx.accounts = await api.renameAccount(id, next);
      flashOk(t("core.account.renamed", { name: next }), 3000);
    } catch (e) {
      accountErr = t("core.account.renameFailed", {
        error: e instanceof Error ? e.message : String(e),
      });
    } finally {
      accountBusy = false;
    }
  }

  async function loadAccountLevels(snap: AccountsResponse) {
    const entries = await Promise.all(
      snap.accounts.map(async (a) => {
        const level = await levelForAccountId(a.id);
        return level != null ? ([a.id, level] as const) : null;
      }),
    );
    const next: Record<string, number> = {};
    for (const e of entries) {
      if (e) next[e[0]] = e[1];
    }
    accountLevels = next;
  }

  async function selectAccount(id: string) {
    if (!id || id === getSelectedAccountId()) return;
    accountBusy = true;
    accountErr = "";
    accountOk = "";
    try {
      ctx.selectedAccountId = id;
      await session.switchAccount(id);
      flashOk(t("settings.accountSwitched"), 3000);
      if (ctx.accounts) void loadAccountLevels(ctx.accounts);
    } catch (e) {
      accountErr = e instanceof Error ? e.message : String(e);
      ctx.selectedAccountId = getSelectedAccountId() || "default";
    } finally {
      accountBusy = false;
    }
  }

  async function createAccount() {
    const name = newAccountName.trim();
    if (!name) return;
    accountBusy = true;
    accountErr = "";
    accountOk = "";
    try {
      const next = await api.createAccount(name);
      ctx.accounts = next;
      newAccountName = "";
      const created = next.createdAccountId;
      if (created) {
        ctx.selectedAccountId = created;
        await session.switchAccount(created);
      }
      flashOk(t("settings.accountCreated"), 3000);
      void loadAccountLevels(next);
    } catch (e) {
      accountErr = e instanceof Error ? e.message : String(e);
    } finally {
      accountBusy = false;
    }
  }

  async function removeAccount(id: string) {
    const accounts = ctx.accounts;
    if (!accounts || id === accounts.defaultAccountId) return;
    const acc = accounts.accounts.find((a) => a.id === id);
    const name = acc?.name || id;
    const ok = await confirmDialog({
      title: t("settings.accountRemove"),
      message: t("settings.accountRemoveConfirm", { name }),
      confirmLabel: t("settings.accountRemove"),
      danger: true,
    });
    if (!ok) return;
    accountBusy = true;
    accountErr = "";
    try {
      const next = await api.deleteAccount(id);
      ctx.accounts = next;
      const current = getSelectedAccountId();
      if (current === id) {
        const fallback = next.defaultAccountId || next.accounts[0]?.id || "default";
        ctx.selectedAccountId = fallback;
        await session.switchAccount(fallback);
      } else {
        ctx.selectedAccountId = current || next.defaultAccountId;
      }
      flashOk(t("settings.accountRemoved"), 3000);
      void loadAccountLevels(next);
    } catch (e) {
      accountErr = e instanceof Error ? e.message : String(e);
    } finally {
      accountBusy = false;
    }
  }

  async function exportProfile() {
    if (!ctx.selectedAccountId) return;
    accountBusy = true;
    accountErr = "";
    accountOk = "";
    try {
      const name = await api.exportAccountProfile(ctx.selectedAccountId);
      flashOk(t("settings.accountExportOk", { name }), 5000);
    } catch (e) {
      accountErr = e instanceof Error ? e.message : String(e);
    } finally {
      accountBusy = false;
    }
  }
</script>

<Panel title={t("settings.panel.account")}>
  <p class="hint">{t("settings.accountHint")}</p>
  {#if shownErr}
    <p class="hint warn">{shownErr}</p>
  {/if}
  {#if accountOk}
    <p class="hint ok">{accountOk}</p>
  {/if}
  {#if !canEdit}
    <p class="hint">{t("core.account.editLocked")}</p>
  {/if}
  {#if ctx.accounts}
    {@const accounts = ctx.accounts}
    <AccountList>
      {#each accounts.accounts as account (account.id)}
        {@const level = levelOf(account.id)}
        {@const isDefault = account.id === accounts.defaultAccountId}
        <AccountRow
          name={account.name}
          selected={account.id === ctx.selectedAccountId}
          current={account.id === currentId}
          currentLabel={t("core.account.current")}
          busy={accountBusy}
          level={level ?? null}
          levelTitle={level != null ? titleForNumericLevel(level) : ""}
          levelLabel={level != null ? t("achievements.levelBadge", { n: level }) : ""}
          defaultBadge={isDefault && level == null ? t("settings.accountDefaultBadge") : ""}
          removeLabel={t("settings.accountRemove")}
          removeDisabled={isDefault || !canEdit}
          removeTitle={isDefault ? t("settings.accountRemoveDisabledDefault") : undefined}
          renameLabel={t("settings.accountRenameCta")}
          renameSaveLabel={t("core.account.renameSave")}
          renameCancelLabel={t("core.account.renameCancel")}
          renamePlaceholder={t("settings.accountRenamePh")}
          onselect={() => void selectAccount(account.id)}
          onremove={() => void removeAccount(account.id)}
          onrename={canEdit ? (name) => renameAccount(account.id, name) : undefined}
        />
      {/each}
    </AccountList>
  {/if}
  <Field label={t("settings.accountNewName")}>
    <TextInput
      bind:value={newAccountName}
      placeholder={t("settings.accountNewPh")}
      disabled={accountBusy}
    />
  </Field>
  <ActionRow>
    <Button disabled={accountBusy || !canEdit || !newAccountName.trim()} onclick={createAccount}>
      {t("settings.accountNew")}
    </Button>
    <Button
      variant="ghost"
      disabled={accountBusy || !ctx.selectedAccountId}
      onclick={exportProfile}
    >
      {t("settings.accountExport")}
    </Button>
  </ActionRow>
</Panel>

<style>
  .hint {
    margin: 0 0 0.85rem;
    color: var(--rk-muted);
  }

  .hint.warn {
    color: var(--rk-danger, #e85d5d);
  }

  .hint.ok {
    color: var(--rk-accent-2, #6bcf8e);
  }
</style>
