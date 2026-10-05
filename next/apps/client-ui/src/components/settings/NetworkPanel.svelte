<script lang="ts">
  import { onMount } from "svelte";
  import { ActionRow, Button, Field, Panel, TextInput } from "@rekord/ui";
  import RemoteAccessPanel from "../RemoteAccessPanel.svelte";
  import { api } from "../../lib/api";
  import { connectGate } from "../../lib/connect.svelte";
  import { t } from "../../lib/i18n.svelte";
  import { session } from "../../lib/session.svelte";
  import type { SettingsAccounts } from "./settingsAccounts.svelte";

  let { ctx }: { ctx: SettingsAccounts } = $props();

  /**
   * The tunnel changes state on its own (starting → running): poll, but only
   * while this panel is on screen and the page is visible — the hub answers
   * this endpoint by shelling out, which is not free.
   */
  const REMOTE_POLL_MS = 15_000;

  let remoteBusy = $state(false);
  let remoteErr = $state("");
  let remoteUrlCopyOk = $state("");
  let remoteUrlCopyTimer: ReturnType<typeof setTimeout> | null = null;

  const canManageMachine = $derived(ctx.canManageMachine);

  async function loadRemote() {
    try {
      ctx.remoteInfo = await api.remoteAccess();
      if (ctx.remoteInfo.status !== "error") remoteErr = "";
      else if (ctx.remoteInfo.error) remoteErr = ctx.remoteInfo.error;
    } catch (e) {
      ctx.remoteInfo = null;
      remoteErr = e instanceof Error ? e.message : String(e);
    }
  }

  onMount(() => {
    void ctx.load();
    void loadRemote();
    let poll: ReturnType<typeof setInterval> | null = null;
    const sync = () => {
      const visible = document.visibilityState === "visible";
      if (visible && poll == null) {
        poll = setInterval(() => void loadRemote(), REMOTE_POLL_MS);
      } else if (!visible && poll != null) {
        clearInterval(poll);
        poll = null;
      }
    };
    const onVisibility = () => {
      // Back on screen: show the current state at once, then keep polling.
      if (document.visibilityState === "visible") void loadRemote();
      sync();
    };
    sync();
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      if (poll != null) clearInterval(poll);
      document.removeEventListener("visibilitychange", onVisibility);
      if (remoteUrlCopyTimer) clearTimeout(remoteUrlCopyTimer);
    };
  });

  const statusLabel = $derived(
    session.hubOffline
      ? t("core.status.offline")
      : session.status === "indexing"
        ? t("core.status.indexing")
        : session.status === "online"
          ? t("core.status.online")
          : "…",
  );

  async function remoteLogin() {
    if (!canManageMachine) return;
    remoteBusy = true;
    remoteErr = "";
    try {
      const d = await api.remoteLogin();
      if (d.loginUrl) window.open(d.loginUrl, "_blank", "noopener,noreferrer");
      await loadRemote();
    } catch (e) {
      remoteErr = e instanceof Error ? e.message : String(e);
    } finally {
      remoteBusy = false;
    }
  }

  async function remoteLogout() {
    if (!canManageMachine) return;
    remoteBusy = true;
    remoteErr = "";
    try {
      ctx.remoteInfo = await api.remoteLogout();
    } catch (e) {
      remoteErr = e instanceof Error ? e.message : String(e);
    } finally {
      remoteBusy = false;
    }
  }

  async function remoteToggleShare() {
    if (!canManageMachine) return;
    remoteBusy = true;
    remoteErr = "";
    try {
      const running =
        ctx.remoteInfo?.status === "running" || ctx.remoteInfo?.status === "starting";
      ctx.remoteInfo = running ? await api.remoteStop() : await api.remoteStart();
      if (ctx.remoteInfo.error) remoteErr = ctx.remoteInfo.error;
    } catch (e) {
      remoteErr = e instanceof Error ? e.message : String(e);
      await loadRemote();
    } finally {
      remoteBusy = false;
    }
  }

  async function copyRemoteUrl(url: string) {
    const value = url.trim();
    if (!value) return;
    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(value);
      } else {
        const ta = document.createElement("textarea");
        ta.value = value;
        ta.setAttribute("readonly", "");
        ta.style.position = "fixed";
        ta.style.left = "-9999px";
        document.body.appendChild(ta);
        ta.select();
        const ok = document.execCommand("copy");
        document.body.removeChild(ta);
        if (!ok) throw new Error("copy failed");
      }
      if (remoteUrlCopyTimer != null) clearTimeout(remoteUrlCopyTimer);
      remoteUrlCopyOk = t("settings.remoteUrlCopied");
      remoteUrlCopyTimer = setTimeout(() => {
        remoteUrlCopyOk = "";
        remoteUrlCopyTimer = null;
      }, 4000);
    } catch {
      remoteErr = t("settings.remoteUrlCopyFailed");
    }
  }
</script>

<Panel title={t("settings.panel.server")}>
  <p class="hint">{t("settings.networkHint")}</p>
  <Field label={t("core.settings.hubAddress")}>
    <TextInput bind:value={session.serverUrl} placeholder="http://127.0.0.1:7420" />
  </Field>
  <ActionRow>
    <Button onclick={() => session.saveServer()}>{t("settings.saveConnect")}</Button>
    <Button variant="ghost" onclick={() => void session.refreshAll()}
      >{t("settings.reload")}</Button
    >
    <Button variant="ghost" onclick={() => connectGate.open()}>{t("settings.changeHub")}</Button>
  </ActionRow>
  <p class="hint">{t("settings.changeHubHint")}</p>
  <p class="hint">
    {t("settings.hubStatus", { status: statusLabel })}
  </p>
  <p class="hint">
    {t("settings.hubPanelHint")}
    {" "}
    <a
      class="rk-link"
      href={session.hubPanelUrl}
      target="_blank"
      rel="noopener noreferrer">{t("settings.openHubPanel")}</a
    >
  </p>
</Panel>
<Panel title={t("settings.panel.remote")}>
  <RemoteAccessPanel
    remote={ctx.remoteInfo}
    busy={remoteBusy}
    error={remoteErr}
    copyOk={remoteUrlCopyOk}
    readOnly={!canManageMachine}
    readOnlyNote={ctx.isDefaultSessionAccount
      ? t("settings.machineOpsHubOnly")
      : t("settings.defaultAccountOnly")}
    hubPanelUrl={ctx.isDefaultSessionAccount ? session.hubPanelUrl : ""}
    onLogin={() => void remoteLogin()}
    onLogout={() => void remoteLogout()}
    onToggleShare={() => void remoteToggleShare()}
    onCopyUrl={(url) => void copyRemoteUrl(url)}
  />
</Panel>

<style>
  .hint {
    margin: 0 0 0.85rem;
    color: var(--rk-muted);
  }
</style>
