<script lang="ts">
  /**
   * Plectr settings (synced per account): note speed as lead time, latency
   * with a tap test, light stage, stage backdrop, key letters, vibration,
   * key layout (presets or one key per lane), challenge mode, records reset.
   */
  import { radioGroupKeys } from "../../lib/radioGroupKeys";
  import { Modal } from "@rekord/ui";
  import LatencyTest from "./LatencyTest.svelte";
  import { t } from "../../lib/i18n.svelte";
  import {
    KEY_PRESETS,
    LANES,
    LATENCY_LIMIT_MS,
    NOTE_SPEED_MAX,
    NOTE_SPEED_MIN,
    type KeyPresetId,
  } from "../../lib/plectr/config";
  import type { PlectrSettings } from "../../lib/plectr/records";
  import { autoLightStage } from "../../lib/plectr/stageQuality";
  import { noteSpeedFor } from "../../lib/plectr/timing";

  let {
    open,
    settings,
    canReset = false,
    onchange,
    onreset,
    onclose,
  }: {
    open: boolean;
    settings: PlectrSettings;
    canReset?: boolean;
    onchange: (patch: Partial<PlectrSettings>) => void;
    onreset: () => void;
    onclose: () => void;
  } = $props();

  let capturing = $state<number | null>(null);
  const autoIsLight = autoLightStage();

  const presetId = $derived(
    (Object.keys(KEY_PRESETS) as KeyPresetId[]).find((id) =>
      KEY_PRESETS[id].every((k, i) => settings.keys[i] === k),
    ) ?? null,
  );

  function keyName(key: string): string {
    const arrows: Record<string, string> = {
      arrowleft: "←",
      arrowright: "→",
      arrowup: "↑",
      arrowdown: "↓",
    };
    return arrows[key] ?? (key.length === 1 ? key.toUpperCase() : key);
  }

  function onCaptureKey(event: KeyboardEvent) {
    const lane = capturing;
    if (lane == null) return;
    event.preventDefault();
    event.stopPropagation();
    const key = event.key.toLowerCase();
    if (key === "escape" || key === " " || key === "tab") {
      capturing = null;
      return;
    }
    const keys = [...settings.keys];
    const other = keys.indexOf(key);
    if (other >= 0 && other !== lane) keys[other] = keys[lane]!;
    keys[lane] = key;
    capturing = null;
    onchange({ keys });
  }
</script>

<svelte:window onkeydowncapture={capturing != null ? onCaptureKey : undefined} />

<Modal {open} title={t("plectr.settings.title")} eyebrow={t("plectr.title")} {onclose}>
  <div class="plectr-settings">
    <section class="plectr-set">
      <label class="plectr-set__label" for="plectr-speed">
        <strong>{t("plectr.settings.speed")}</strong>
        <span>{t("plectr.settings.speedValue", { x: settings.speed.toFixed(2), px: Math.round(noteSpeedFor(settings.speed)) })}</span>
      </label>
      <input
        id="plectr-speed"
        type="range"
        min={NOTE_SPEED_MIN}
        max={NOTE_SPEED_MAX}
        step="0.05"
        value={settings.speed}
        oninput={(e) => onchange({ speed: Number(e.currentTarget.value) })}
      />
      <small>{t("plectr.settings.speedHint")}</small>
    </section>

    <section class="plectr-set">
      <label class="plectr-set__label" for="plectr-latency">
        <strong>{t("plectr.settings.latency")}</strong>
        <span>{t("plectr.settings.latencyValue", { ms: settings.latencyMs })}</span>
      </label>
      <input
        id="plectr-latency"
        type="range"
        min={-LATENCY_LIMIT_MS}
        max={LATENCY_LIMIT_MS}
        step="5"
        value={settings.latencyMs}
        oninput={(e) => onchange({ latencyMs: Number(e.currentTarget.value) })}
      />
      <LatencyTest onresult={(ms) => onchange({ latencyMs: ms })} />
    </section>

    <section class="plectr-set">
      <span class="plectr-set__label"><strong>{t("plectr.settings.light")}</strong></span>
      <div class="plectr-seg" role="radiogroup" use:radioGroupKeys aria-label={t("plectr.settings.light")}>
        {#each ["auto", "on", "off"] as const as mode (mode)}
          <button
            type="button"
            role="radio"
            aria-checked={settings.lightStage === mode}
            class:is-on={settings.lightStage === mode}
            onclick={() => onchange({ lightStage: mode })}>{t(`plectr.settings.light_${mode}`)}</button
          >
        {/each}
      </div>
      <small>{autoIsLight ? t("plectr.settings.lightAutoOn") : t("plectr.settings.lightAutoOff")}</small>
    </section>

    <section class="plectr-set">
      <span class="plectr-set__label"><strong>{t("plectr.settings.backdrop")}</strong></span>
      <div class="plectr-seg" role="radiogroup" use:radioGroupKeys aria-label={t("plectr.settings.backdrop")}>
        {#each ["off", "bars", "art"] as const as mode (mode)}
          <button
            type="button"
            role="radio"
            aria-checked={settings.backdrop === mode}
            class:is-on={settings.backdrop === mode}
            onclick={() => onchange({ backdrop: mode })}>{t(`plectr.settings.backdrop_${mode}`)}</button
          >
        {/each}
      </div>
      <small>{t("plectr.settings.backdropHint")}</small>
    </section>

    <section class="plectr-set">
      <span class="plectr-set__label"><strong>{t("plectr.settings.keys")}</strong></span>
      <div class="plectr-seg" role="radiogroup" use:radioGroupKeys aria-label={t("plectr.settings.keys")}>
        {#each Object.keys(KEY_PRESETS) as KeyPresetId[] as id (id)}
          <button
            type="button"
            role="radio"
            aria-checked={presetId === id}
            class:is-on={presetId === id}
            onclick={() => onchange({ keys: [...KEY_PRESETS[id]] })}>{t(`plectr.settings.keys_${id}`)}</button
          >
        {/each}
      </div>
      <div class="plectr-keymap">
        {#each LANES as lane, i (lane.name)}
          <button
            type="button"
            class="plectr-keymap__key"
            class:is-capturing={capturing === i}
            style="--lane-color: {lane.color}"
            aria-label={t("plectr.settings.keyFor", { n: i + 1 })}
            onclick={() => (capturing = capturing === i ? null : i)}
          >
            {capturing === i ? "…" : keyName(settings.keys[i] ?? "")}
          </button>
        {/each}
      </div>
      <small>{capturing != null ? t("plectr.settings.keyCapture") : t("plectr.settings.keysHint")}</small>
    </section>

    <section class="plectr-set plectr-set--toggles">
      <label class="plectr-switch">
        <input type="checkbox" checked={settings.keyLetters} onchange={(e) => onchange({ keyLetters: e.currentTarget.checked })} />
        <span><strong>{t("plectr.settings.keyLetters")}</strong></span>
      </label>
      <label class="plectr-switch">
        <input type="checkbox" checked={settings.vibration} onchange={(e) => onchange({ vibration: e.currentTarget.checked })} />
        <span>
          <strong>{t("plectr.settings.vibration")}</strong>
          <small>{t("plectr.settings.vibrationHint")}</small>
        </span>
      </label>
      <label class="plectr-switch">
        <input type="checkbox" checked={settings.challenge} onchange={(e) => onchange({ challenge: e.currentTarget.checked })} />
        <span>
          <strong>{t("plectr.settings.challenge")}</strong>
          <small>{t("plectr.settings.challengeHint")}</small>
        </span>
      </label>
    </section>

    <section class="plectr-set">
      <button type="button" class="rk-btn rk-btn--ghost rk-btn--danger" disabled={!canReset} onclick={onreset}>
        {t("plectr.reset.action")}
      </button>
    </section>
  </div>
</Modal>
