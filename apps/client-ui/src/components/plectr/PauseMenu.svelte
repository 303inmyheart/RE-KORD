<script lang="ts">
  /**
   * The song is paused (from here, the dock or media keys) or Plectr opened on
   * a paused song: a card over the lanes. Resume plays the song again at once.
   */
  import { radioGroupKeys } from "../../lib/radioGroupKeys";
  import UiIcon from "../icons/UiIcon.svelte";
  import { t } from "../../lib/i18n.svelte";
  import { DIFFICULTIES } from "../../lib/plectr/config";
  import type { DifficultyId } from "../../lib/plectr/types";
  import type { PauseReason } from "./GameStage.svelte";

  let {
    reason = "user",
    difficulty,
    playable,
    onresume,
    onrestart,
    onchange,
    ondifficulty,
    onsettings,
    onexit,
  }: {
    reason?: PauseReason;
    difficulty: DifficultyId;
    playable: DifficultyId[];
    onresume: () => void;
    onrestart: () => void;
    onchange: () => void;
    ondifficulty: (id: DifficultyId) => void;
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
      {reason === "idle" ? t("plectr.pick.play") : t("plectr.resume")}
    </button>
    <div class="plectr-pause__row">
      <button type="button" class="rk-btn rk-btn--secondary" onclick={onrestart}>
        <UiIcon name="repeat" />
        {t("plectr.restart")}
      </button>
      <button type="button" class="rk-btn rk-btn--secondary" onclick={onchange}>
        <UiIcon name="queueMusic" />
        {t("plectr.changeTrack")}
      </button>
    </div>
    <div class="plectr-diff plectr-diff--small" role="radiogroup" use:radioGroupKeys aria-label={t("plectr.difficulty")}>
      {#each DIFFICULTIES as d (d.id)}
        <button
          type="button"
          role="radio"
          aria-checked={difficulty === d.id}
          class="plectr-diff__opt plectr-diff__opt--{d.id}"
          class:is-on={difficulty === d.id}
          disabled={!playable.includes(d.id)}
          onclick={() => ondifficulty(d.id)}
        >
          <span>{t(`plectr.diff.${d.id}`)}</span>
          <small>{t("plectr.level", { n: d.level })}</small>
        </button>
      {/each}
    </div>
    <div class="plectr-pause__row">
      <button type="button" class="rk-btn rk-btn--ghost" onclick={onsettings}>
        <UiIcon name="settings" />
        {t("plectr.settings.open")}
      </button>
      <button type="button" class="rk-btn rk-btn--ghost" onclick={onexit}>
        <UiIcon name="close" />
        {t("plectr.exit")}
      </button>
    </div>
  </div>
</div>
