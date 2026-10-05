<script lang="ts">
  import { Button, IconButton } from "@rekord/ui";
  import { t } from "../lib/i18n.svelte";
  import { player } from "../lib/player";
  import { sleepCountdown } from "../lib/sleepCountdown.svelte";

  let open = $state(false);
  let custom = $state("45");

  const countdown = sleepCountdown();
  const active = $derived(countdown.active);

  function start(minutes: number) {
    player.setSleepTimer(minutes);
    countdown.refresh();
    open = false;
  }
</script>

<div class="wrap">
  <IconButton
    label={t("listen.sleepTimer")}
    active={active}
    onclick={() => (open = !open)}
  >
    ⏱
  </IconButton>
  {#if open}
    <div class="pop">
      <strong>
        {active
          ? t("core.sleep.remaining", { time: countdown.label })
          : t("listen.sleepTimer")}
      </strong>
      <div class="row">
        {#each [15, 30, 60] as m}
          <Button variant="ghost" onclick={() => start(m)}>
            {t("core.sleep.minutesShort", { count: m })}
          </Button>
        {/each}
      </div>
      <div class="row">
        <input
          type="number"
          min="1"
          max="600"
          bind:value={custom}
          aria-label={t("core.sleep.customMinutes")}
        />
        <Button
          onclick={() => {
            const n = Number(custom);
            if (n > 0) start(n);
          }}
        >
          {t("listen.sleepTimerStart")}
        </Button>
      </div>
      {#if active}
        <Button variant="ghost" onclick={() => { player.setSleepTimer(null); open = false; }}>
          {t("listen.sleepTimerCancel")}
        </Button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .wrap {
    position: relative;
  }

  .pop {
    position: absolute;
    right: 0;
    bottom: calc(100% + 8px);
    z-index: var(--rk-z-popover);
    width: 12rem;
    display: grid;
    gap: 0.45rem;
    padding: 0.65rem;
    background: var(--rk-surface);
    border: 1px solid var(--rk-line);
    border-radius: var(--rk-radius);
    box-shadow: var(--rk-shadow);
  }

  .row {
    display: flex;
    gap: 0.35rem;
    align-items: center;
  }

  input {
    width: 4rem;
    border: 1px solid var(--rk-line);
    background: var(--rk-surface-3);
    color: inherit;
    border-radius: var(--rk-radius-sm);
    padding: 0.3rem;
    font: inherit;
  }

  strong {
    font-size: var(--rk-fs-sm);
  }
</style>
