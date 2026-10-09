<script lang="ts">
  import { Banner, Button, Field, Panel, Segmented } from "@rekord/ui";
  import type { PowerState, PreventSleepMode } from "../api";
  import { admin } from "../lib/admin.svelte";
  import { formatClock, t } from "../lib/i18n.svelte";
  import { powerLidLine, powerStatusLine } from "../lib/powerStatus";

  let { power }: { power: PowerState } = $props();

  const MODES: PreventSleepMode[] = ["off", "always", "whenActive"];

  const locked = $derived(admin.busy || !admin.canManage);
  const modeOptions = $derived(
    MODES.map((m) => ({
      value: m,
      label: t(`power.mode.${m}`),
      disabled: locked || power.lockedByEnv,
    })),
  );
  const line = $derived(powerStatusLine(power, t, formatClock));
  const lidLine = $derived(powerLidLine(power, t));
  /** The lid option exists only where the platform can block it (Linux). */
  const showLid = $derived(power.status.lidSupported);

  let grace = $state(10);
  $effect.pre(() => {
    grace = power.graceMinutes;
  });

  function setMode(value: string) {
    if (value === power.preventSleep) return;
    void admin.setPower({ preventSleep: value as PreventSleepMode });
  }

  function saveGrace() {
    const n = Math.round(Number(grace));
    if (!Number.isFinite(n)) return;
    void admin.setPower({ graceMinutes: n });
  }
</script>

<Panel title={t("power.title")}>
  {#snippet actions()}
    <Button variant="secondary" disabled={admin.busy} onclick={() => void admin.loadPower()}>
      {t("common.refresh")}
    </Button>
  {/snippet}

  <p class="hint">{t("power.hint")}</p>

  <div class="mode">
    <span class="k">{t("power.mode.label")}</span>
    <Segmented
      ariaLabel={t("power.mode.label")}
      options={modeOptions}
      value={power.preventSleep}
      onchange={setMode}
    />
  </div>
  <p class="hint">{t(`power.mode.hint.${power.preventSleep}`)}</p>
  {#if power.lockedByEnv}
    <Banner tone="info">{t("power.lockedByEnv")}</Banner>
  {/if}

  {#if power.preventSleep === "whenActive"}
    <div class="grace">
      <Field label={t("power.grace.label")}>
        <input
          class="rk-input num"
          type="number"
          min={power.limits.minGraceMinutes}
          max={power.limits.maxGraceMinutes}
          step="1"
          bind:value={grace}
          disabled={locked}
          onkeydown={(e) => {
            if (e.key === "Enter") saveGrace();
          }}
        />
      </Field>
      <Button
        variant="secondary"
        disabled={locked || grace === power.graceMinutes}
        onclick={saveGrace}
      >
        {t("common.save")}
      </Button>
    </div>
    <p class="hint">
      {t("power.grace.hint", {
        min: power.limits.minGraceMinutes,
        max: power.limits.maxGraceMinutes,
      })}
    </p>
  {/if}

  {#if showLid}
    <label class="check">
      <input
        type="checkbox"
        checked={power.keepAwakeLidClosed}
        disabled={locked}
        onchange={(e) => void admin.setPower({ keepAwakeLidClosed: e.currentTarget.checked })}
      />
      <span>{t("power.lid.label")}</span>
    </label>
    <p class="hint lid">{t("power.lid.hint")}</p>
  {/if}

  <p class="status" data-tone={line.tone} role="status">
    <span class="dot" aria-hidden="true"></span>
    <span>{line.text}</span>
  </p>
  {#if lidLine}
    <p class="hint warn">{lidLine}</p>
  {/if}
  {#if !power.status.supported}
    <Banner tone="info">{t("power.unsupported")}</Banner>
  {:else if power.status.method}
    <p class="meta">{t("power.method", { method: power.status.method })}</p>
  {/if}
</Panel>

<style>
  .hint {
    margin: 0.4rem 0 0.8rem;
    color: var(--rk-muted);
    font-size: var(--rk-fs-sm);
    line-height: var(--rk-lh);
  }

  .hint.lid {
    margin-top: 0.2rem;
    padding-left: 1.6rem;
  }

  .hint.warn {
    color: var(--rk-warning);
  }

  .mode {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem 0.9rem;
  }

  .k {
    font-size: var(--rk-fs-xs);
    color: var(--rk-muted);
  }

  .grace {
    display: flex;
    align-items: flex-end;
    gap: 0.6rem;
    max-width: 22rem;
  }

  .grace :global(.num) {
    width: 6.5rem;
  }

  .grace :global(.rk-field) {
    margin-bottom: 0;
  }

  .check {
    display: flex;
    align-items: flex-start;
    gap: 0.55rem;
    margin: 0.8rem 0 0;
    line-height: var(--rk-lh);
  }

  .check input {
    margin-top: 0.2rem;
  }

  .status {
    display: flex;
    align-items: baseline;
    gap: 0.55rem;
    margin: 1rem 0 0.4rem;
    font-weight: 600;
    line-height: var(--rk-lh);
  }

  .dot {
    flex: none;
    width: 0.6rem;
    height: 0.6rem;
    border-radius: 50%;
    background: var(--rk-muted);
    transform: translateY(-0.05rem);
  }

  .status[data-tone="on"] .dot {
    background: var(--rk-ok);
  }

  .status[data-tone="error"] .dot {
    background: var(--rk-danger);
  }

  .meta {
    margin: 0;
    font-size: var(--rk-fs-xs);
    color: var(--rk-muted);
  }
</style>
