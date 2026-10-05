<script lang="ts">
  import { Button } from "@rekord/ui";
  import { t } from "../lib/i18n.svelte";
  import UiIcon from "./icons/UiIcon.svelte";

  let {
    label,
    disabled = false,
    onclick,
  }: {
    label?: string;
    disabled?: boolean;
    onclick?: () => void;
  } = $props();
</script>

<Button class="play-collection-btn" {disabled} {onclick}>
  <UiIcon name="shuffle" />
  {label ?? t("player.shuffle")}
</Button>
