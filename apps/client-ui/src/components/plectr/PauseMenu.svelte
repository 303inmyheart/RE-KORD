<script lang="ts">
  /**
   * The song is paused (from here, the dock or media keys) or Plectr opened on
   * a paused song: a card over the lanes. Resume plays the song again at once;
   * Exit leaves Plectr (the music keeps its state).
   */
  import UiIcon from "../icons/UiIcon.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { PauseReason } from "./GameStage.svelte";

  let {
    reason = "user",
    onresume,
    onrestart,
    onsettings,
    onexit,
  }: {
    reason?: PauseReason;
    onresume: () => void;
    onrestart: () => void;
    onsettings: () => void;
    onexit: () => void;
  } = $props();

  let resumeBtn = $state<HTMLButtonElement | null>(null);
  $effect(() => {
    resumeBtn?.focus({ preventScroll: true });
  });
</script>

<div class="plectr-pause plectr-overlay" role="dialog" aria-modal="true" aria-labelledby="plectr-pause-title">
  <div class="plectr-pause__card">
    <h3 id="plectr-pause-title">{reason === "idle" ? t("plectr.paused.ready") : t("plectr.paused.title")}</h3>
    {#if reason !== "user"}
      <p class="plectr-pause__hint">{t(`plectr.paused.reason.${reason}`)}</p>
    {/if}
    <button bind:this={resumeBtn} type="button" class="rk-btn rk-btn--primary plectr-pause__main" onclick={onresume}>
      <UiIcon name="play" />
      {reason === "idle" ? t("plectr.play") : t("plectr.resume")}
    </button>
    <div class="plectr-pause__row">
      <button type="button" class="rk-btn rk-btn--secondary" onclick={onrestart}>
        <UiIcon name="repeat" />
        {t("plectr.restart")}
      </button>
      <button type="button" class="rk-btn rk-btn--secondary" onclick={onsettings}>
        <UiIcon name="settings" />
        {t("plectr.settings.open")}
      </button>
    </div>
    <button type="button" class="rk-btn rk-btn--ghost plectr-pause__exit" onclick={onexit}>
      <UiIcon name="close" />
      {t("plectr.exit")}
    </button>
  </div>
</div>
