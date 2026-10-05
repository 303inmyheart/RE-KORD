/**
 * State the Settings panels share: the account list (several panels gate
 * host-level actions on "is this the default account?") and the last remote
 * access snapshot (Network shows it, System copies it into the report).
 *
 * One instance per SettingsView mount, handed to the panels as a prop.
 */

import { getSelectedAccountId, type AccountsResponse } from "../../lib/account";
import { api, type RemoteAccessState } from "../../lib/api";
import type { LegacyImportReport } from "../../lib/legacyImport";
import { session } from "../../lib/session.svelte";

/**
 * Long-running System jobs (backup, restore, legacy import). They outlive the
 * panel: switching tab mid-way and coming back still shows their progress, and
 * the Interface panel keeps its theme import/export disabled meanwhile.
 */
export class SettingsJobs {
  backupBusy = $state(false);
  restoreBusy = $state(false);
  backupOk = $state("");
  backupErr = $state("");
  restoreOk = $state("");
  restoreErr = $state("");
  importBusy = $state(false);
  importPhase = $state("");
  importError = $state("");
  importReport = $state<LegacyImportReport | null>(null);
  importSource = $state("");

  get hubBusy(): boolean {
    return this.backupBusy || this.restoreBusy;
  }
}

export class SettingsAccounts {
  accounts = $state<AccountsResponse | null>(null);
  selectedAccountId = $state(getSelectedAccountId() || "default");
  /** Last error from loading the list (Account panel shows it). */
  loadError = $state("");
  remoteInfo = $state<RemoteAccessState | null>(null);
  readonly jobs = new SettingsJobs();

  /** Hub integrations + remote access are owned by the default account (`default` / Locale). */
  readonly defaultAccountId = $derived(this.accounts?.defaultAccountId || "default");
  readonly isDefaultSessionAccount = $derived(this.selectedAccountId === this.defaultAccountId);
  /**
   * Host-level writes (library path, scans, credentials, restores, tunnel) live
   * in the hub panel: from here they are read-only unless the hub says this
   * client may run them.
   */
  readonly canManageMachine = $derived(
    this.isDefaultSessionAccount && session.canManageMachine,
  );
  /** Library operations, account create / rename / delete included (any local account). */
  readonly canManageLibrary = $derived(session.canManageLibrary);

  private inFlight: Promise<AccountsResponse | null> | null = null;

  /** Fetch the list; concurrent callers share one request. */
  load(): Promise<AccountsResponse | null> {
    if (this.inFlight) return this.inFlight;
    this.inFlight = (async () => {
      try {
        const data = await api.ensureAccountSession();
        this.accounts = data;
        this.selectedAccountId = getSelectedAccountId() || data.defaultAccountId;
        this.loadError = "";
        return data;
      } catch (e) {
        this.loadError = e instanceof Error ? e.message : String(e);
        return null;
      } finally {
        this.inFlight = null;
      }
    })();
    return this.inFlight;
  }
}
