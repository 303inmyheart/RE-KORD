/**
 * May this client change the library from Studio (download, metadata, covers,
 * curiosità)? Read from the hub's `system/machine-access`, so the panes can say
 * why a control is off instead of failing with a 403 at the last step.
 */
import { session } from "../session.svelte";
import { t } from "../i18n.svelte";

type Access = {
  isDefaultAccount?: boolean;
  local?: boolean;
  allowRemoteAdmin?: boolean;
  canManageMachine?: boolean;
  /** Newer hubs: library writes (Studio) may be wider than machine ops. */
  canManageLibrary?: boolean;
  /** `forbidden_remote` / `forbidden_default_account` when denied. */
  libraryDeniedReason?: string | null;
  machineDeniedReason?: string | null;
};

function reasonText(code: string | null | undefined, a: Access | null): string {
  if (code === "forbidden_default_account") return t("studio.access.needDefault");
  if (code === "forbidden_remote") return t("studio.access.needLocal");
  if (a && a.local === false && !a.allowRemoteAdmin) return t("studio.access.needLocal");
  if (a && a.isDefaultAccount === false) return t("studio.access.needDefault");
  return t("studio.access.generic");
}

class StudioAccess {
  private get raw(): Access | null {
    return (session.machineAccess as Access | null) ?? null;
  }

  /** Library writes (Studio). Unknown rights stay optimistic. */
  readonly canWrite = $derived.by(() => {
    const a = this.raw;
    if (!a) return true;
    if (typeof a.canManageLibrary === "boolean") return a.canManageLibrary;
    return a.canManageMachine !== false;
  });

  /** Hub tools (update yt-dlp). */
  readonly canManageMachine = $derived(this.raw?.canManageMachine !== false);

  /** Why writes are off, in plain words; null when allowed. */
  readonly reason = $derived.by(() =>
    this.canWrite ? null : reasonText(this.raw?.libraryDeniedReason, this.raw),
  );

  /** Why the yt-dlp update is off (machine operation). */
  readonly machineReason = $derived.by(() =>
    this.canManageMachine ? null : reasonText(this.raw?.machineDeniedReason, this.raw),
  );

  private loading: Promise<void> | null = null;

  /** Fetch rights once per Studio visit when the session has not yet. */
  ensure() {
    if (session.machineAccess || this.loading) return;
    this.loading = session.loadMachineAccess().finally(() => {
      this.loading = null;
    });
  }

  /** Resolves once the rights are known (or could not be read). */
  async ready(): Promise<void> {
    this.ensure();
    if (this.loading) await this.loading.catch(() => {});
  }
}

export const studioAccess = new StudioAccess();
