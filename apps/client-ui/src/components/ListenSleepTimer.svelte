<script lang="ts">
  import UiIcon from "./icons/UiIcon.svelte";
  import { t } from "../lib/i18n.svelte";
  import { player } from "../lib/player";
  import { sleepCountdown } from "../lib/sleepCountdown.svelte";

  const PRESETS = [15, 30, 60] as const;

  let open = $state(false);
  let customHours = $state("0");
  let customMinutes = $state("45");
  let customError = $state(false);

  const countdown = sleepCountdown();
  const active = $derived(countdown.active);
  const remainingLabel = $derived(countdown.label);

  function digitsOnly(value: string): string {
    return value.replace(/\D/g, "");
  }

  function presetLabel(min: (typeof PRESETS)[number]): string {
    if (min === 15) return t("player.sleepTimer15");
    if (min === 30) return t("player.sleepTimer30");
    return t("player.sleepTimer60");
  }

  function startTimer(minutes: number) {
    player.setSleepTimer(minutes);
    countdown.refresh();
    customError = false;
    open = false;
  }

  function startCustom() {
    const h = Number(customHours) || 0;
    const m = Number(customMinutes) || 0;
    const total = h * 60 + m;
    if (total <= 0 || h > 12) {
      customError = true;
      return;
    }
    startTimer(total);
  }
</script>

<section
  class="listen-sleep-timer"
  class:is-open={open}
  class:is-active={active}
  aria-label={t("listen.sleepTimerAria")}
>
  <button
    type="button"
    class="listen-sleep-timer__toggle"
    onclick={() => (open = !open)}
    aria-expanded={open}
  >
    <span class="listen-sleep-timer__toggle-main">
      <UiIcon name="history" class="listen-sleep-timer__ic" />
      <span class="listen-sleep-timer__toggle-title">{t("listen.sleepTimer")}</span>
      {#if active}
        <span class="listen-sleep-timer__badge" aria-live="polite">{remainingLabel}</span>
      {/if}
    </span>
    <UiIcon
      name="chevronRight"
      class="listen-sleep-timer__chev{open ? ' is-open' : ''}"
    />
  </button>

  {#if open}
    <div class="listen-sleep-timer__panel">
      <div class="listen-sleep-timer__row">
        {#each PRESETS as min}
          <button
            type="button"
            class="ghost-btn ghost-btn--sm"
            onclick={() => startTimer(min)}
          >
            {presetLabel(min)}
          </button>
        {/each}

        <label class="listen-sleep-timer__field">
          <span class="listen-sleep-timer__field-label">{t("listen.sleepTimerHours")}</span>
          <input
            type="text"
            class="ghost-input listen-sleep-timer__input"
            inputmode="numeric"
            autocomplete="off"
            aria-label={t("listen.sleepTimerHours")}
            value={customHours}
            oninput={(e) => {
              customError = false;
              customHours = digitsOnly(e.currentTarget.value);
            }}
          />
        </label>

        <label class="listen-sleep-timer__field">
          <span class="listen-sleep-timer__field-label">{t("listen.sleepTimerMinutes")}</span>
          <input
            type="text"
            class="ghost-input listen-sleep-timer__input"
            inputmode="numeric"
            autocomplete="off"
            aria-label={t("listen.sleepTimerMinutes")}
            value={customMinutes}
            oninput={(e) => {
              customError = false;
              customMinutes = digitsOnly(e.currentTarget.value);
            }}
            onkeydown={(e) => {
              if (e.key === "Enter") startCustom();
            }}
          />
        </label>

        <button type="button" class="ghost-btn ghost-btn--sm" onclick={startCustom}>
          {t("listen.sleepTimerStart")}
        </button>

        {#if active}
          <button
            type="button"
            class="text-btn listen-sleep-timer__cancel"
            onclick={() => player.setSleepTimer(null)}
          >
            {t("listen.sleepTimerCancel")}
          </button>
        {/if}
      </div>
      {#if customError}
        <p class="listen-sleep-timer__error warnline" role="alert">
          {t("listen.sleepTimerInvalid")}
        </p>
      {/if}
    </div>
  {/if}
</section>
