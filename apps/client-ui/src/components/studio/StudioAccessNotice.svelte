<script lang="ts">
  import { studioAccess } from "../../lib/studio/access.svelte";
  import { t } from "../../lib/i18n.svelte";

  /** Shown on top of a Studio pane whose write controls are disabled. */
  let { what }: { what?: string } = $props();
</script>

{#if !studioAccess.canWrite}
  <div class="studio-access-notice" role="note">
    <svg class="studio-access-notice__ic" viewBox="0 0 24 24" aria-hidden="true">
      <path
        fill="currentColor"
        d="M12 1a5 5 0 0 0-5 5v3H6a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V11a2 2 0 0 0-2-2h-1V6a5 5 0 0 0-5-5Zm-3 8V6a3 3 0 1 1 6 0v3H9Z"
      />
    </svg>
    <div class="studio-access-notice__text">
      <strong>{what ?? t("studio.access.readOnly")}</strong>
      <span>{studioAccess.reason}</span>
    </div>
  </div>
{/if}
