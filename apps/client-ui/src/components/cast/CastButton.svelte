<script lang="ts">
  /**
   * Cast toggle for the player dock. Renders nothing where casting is not
   * possible (Tauri webviews, non-Chrome browsers, plain-http pages, no backend).
   */
  import { IconButton } from "@rekord/ui";
  import { castController } from "../../lib/cast/castController.svelte";
  import { t } from "../../lib/i18n.svelte";
  import CastIcon from "./CastIcon.svelte";

  const connected = $derived(castController.connected);
  const connecting = $derived(castController.status.session === "connecting" || castController.busy);
  const label = $derived(castButtonLabel(connected, connecting));

  function castButtonLabel(on: boolean, pending: boolean) {
    if (on) return t("cast.stopOn", { name: castController.status.deviceName ?? t("cast.device") });
    return pending ? t("cast.connecting") : t("cast.start");
  }

  function onclick() {
    if (connected) void castController.disconnect();
    else void castController.connect();
  }
</script>

{#if castController.available}
  <IconButton {label} active={connected} {onclick}>
    <CastIcon {connected} {connecting} />
  </IconButton>
{/if}
