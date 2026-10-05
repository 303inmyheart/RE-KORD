<script lang="ts">
  /**
   * Tap test: 12 clicks at a steady beat, tap on each one (button, Space or
   * any lane key). The median offset (first taps dropped) is the latency.
   */
  import { onDestroy } from "svelte";
  import { t } from "../../lib/i18n.svelte";
  import { estimateLatencyMs } from "../../lib/plectr/timing";

  let { onresult }: { onresult: (ms: number) => void } = $props();

  const BEATS = 12;
  const INTERVAL = 0.6;

  let running = $state(false);
  let beat = $state(0);
  let result = $state<number | null>(null);
  let failed = $state(false);
  let ctx: AudioContext | null = null;
  let clicks: number[] = [];
  let taps: number[] = [];
  let timer = 0;

  function stop() {
    running = false;
    window.clearInterval(timer);
    timer = 0;
    void ctx?.close().catch(() => {});
    ctx = null;
  }

  async function start() {
    stop();
    result = null;
    failed = false;
    taps = [];
    clicks = [];
    beat = 0;
    try {
      ctx = new AudioContext();
      await ctx.resume();
    } catch {
      failed = true;
      return;
    }
    const c = ctx;
    const perf0 = performance.now();
    const t0 = c.currentTime + 0.5;
    for (let i = 0; i < BEATS; i += 1) {
      const at = t0 + i * INTERVAL;
      const osc = c.createOscillator();
      const gain = c.createGain();
      osc.frequency.value = i % 4 === 0 ? 1320 : 880;
      gain.gain.setValueAtTime(0.0001, at);
      gain.gain.exponentialRampToValueAtTime(0.5, at + 0.004);
      gain.gain.exponentialRampToValueAtTime(0.0001, at + 0.08);
      osc.connect(gain).connect(c.destination);
      osc.start(at);
      osc.stop(at + 0.1);
      clicks.push(perf0 + (at - c.currentTime) * 1000);
    }
    running = true;
    timer = window.setInterval(() => {
      const now = performance.now();
      beat = clicks.filter((p) => p <= now).length;
      if (now > clicks[clicks.length - 1]! + 700) finish();
    }, 50);
  }

  function tap() {
    if (!running) return;
    const now = performance.now();
    let best = clicks[0]!;
    for (const c of clicks) if (Math.abs(now - c) < Math.abs(now - best)) best = c;
    taps.push(now - best);
  }

  function finish() {
    stop();
    const ms = estimateLatencyMs(taps);
    if (ms == null) {
      failed = true;
      return;
    }
    result = ms;
    onresult(ms);
  }

  function onKey(event: KeyboardEvent) {
    if (!running || event.repeat) return;
    if (event.key === "Escape") return;
    event.preventDefault();
    tap();
  }

  onDestroy(stop);
</script>

<svelte:window onkeydown={onKey} />

<div class="plectr-latency">
  {#if running}
    <button type="button" class="plectr-latency__pad" onpointerdown={(e) => { e.preventDefault(); tap(); }}>
      <span>{t("plectr.latency.tap")}</span>
      <small>{t("plectr.latency.progress", { n: beat, total: BEATS })}</small>
    </button>
  {:else}
    <button type="button" class="rk-btn rk-btn--secondary" onclick={() => void start()}>
      {t("plectr.latency.start")}
    </button>
    {#if result != null}
      <p class="plectr-latency__msg">{t("plectr.latency.result", { ms: result })}</p>
    {:else if failed}
      <p class="plectr-latency__msg">{t("plectr.latency.failed")}</p>
    {:else}
      <p class="plectr-latency__msg">{t("plectr.latency.hint")}</p>
    {/if}
  {/if}
</div>
