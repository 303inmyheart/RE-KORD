<script lang="ts">
  /** Right panel (desktop) / info sheet (tablet): the run in progress — accuracy ring, judgements, combo meter, keys. */
  import { fmtNumber, t } from "../../lib/i18n.svelte";
  import { LANES } from "../../lib/plectr/config";
  import { comboMultiplier } from "../../lib/plectr/engine";
  import { resultGrade } from "../../lib/plectr/runResult";
  import type { LiveStats } from "./GameStage.svelte";

  let {
    stats,
    keys,
    live = false,
  }: { stats: LiveStats | null; keys: string[]; live?: boolean } = $props();

  const RING = 2 * Math.PI * 40;
  const pct = (n: number) => `${Math.round(n * 1000) / 10}%`;
  const s = $derived(
    stats ?? {
      perfects: 0,
      goods: 0,
      earlies: 0,
      lates: 0,
      misses: 0,
      skipped: 0,
      combo: 0,
      maxCombo: 0,
      judged: 0,
      totalNotes: 0,
      accuracy: 0,
      score: 0,
    },
  );
  const mult = $derived(comboMultiplier(s.combo));
  const toNext = $derived(mult >= 4 ? 1 : (s.combo % 12) / 12);
  const grade = $derived(s.judged ? resultGrade(s.accuracy, false) : null);
  const progress = $derived(s.totalNotes ? Math.min(1, (s.judged + s.skipped) / s.totalNotes) : 0);
  const rows = $derived([
    { key: "perfect", label: t("plectr.results.perfect"), n: s.perfects },
    { key: "good", label: t("plectr.results.good"), n: s.goods },
    { key: "timing", label: t("plectr.results.earlyLate"), n: s.earlies + s.lates },
    { key: "miss", label: t("plectr.results.miss"), n: s.misses },
  ]);
  const max = $derived(Math.max(1, ...rows.map((r) => r.n)));

  function keyName(key: string): string {
    const arrows: Record<string, string> = { arrowleft: "←", arrowright: "→", arrowup: "↑", arrowdown: "↓" };
    return arrows[key] ?? key.toUpperCase();
  }
</script>

<section class="plectr-panel plectr-panel--session rk-surface-card" aria-label={t("plectr.panel.session")}>
  <header class="plectr-panel__head">
    <h3 class="plectr-panel__heading">{t("plectr.panel.session")}</h3>
    <span class="plectr-live-dot" class:is-on={live} aria-hidden="true"></span>
  </header>

  <div class="plectr-live__hero">
    <div class="plectr-live__ring" role="img" aria-label={`${t("plectr.results.accuracy")} ${s.judged ? pct(s.accuracy) : "—"}`}>
      <svg viewBox="0 0 96 96" aria-hidden="true">
        <circle cx="48" cy="48" r="40" class="plectr-ring__track" />
        <circle
          cx="48"
          cy="48"
          r="40"
          class="plectr-ring__value"
          stroke-dasharray={RING}
          stroke-dashoffset={RING * (1 - (s.judged ? s.accuracy : 0))}
        />
      </svg>
      <span>
        <strong>{s.judged ? pct(s.accuracy) : "—"}</strong>
        <small>{t("plectr.results.accuracy")}</small>
      </span>
    </div>
    <dl class="plectr-live__facts">
      <div>
        <dt>{t("plectr.hudScore")}</dt>
        <dd>{fmtNumber(s.score)}</dd>
      </div>
      <div>
        <dt>{t("plectr.panel.grade")}</dt>
        <dd>
          {#if grade}<span class="plectr-grade plectr-grade--{grade.toLowerCase()}">{grade}</span>{:else}—{/if}
        </dd>
      </div>
    </dl>
  </div>

  <ul class="plectr-live__rows">
    {#each rows as r (r.key)}
      <li class="is-{r.key}">
        <span>{r.label}</span>
        <i style="transform: scaleX({r.n / max})"></i>
        <strong>{fmtNumber(r.n)}</strong>
      </li>
    {/each}
  </ul>

  <div class="plectr-live__combo">
    <span>{t("plectr.hudCombo")} <strong>×{s.combo}</strong></span>
    <span class="plectr-live__mult">{t("plectr.multiplier", { n: mult })}</span>
    <div class="plectr-live__meter" aria-hidden="true"><i style="transform: scaleX({toNext})"></i></div>
    <small>{t("plectr.results.maxCombo")} ×{s.maxCombo}</small>
  </div>

  <div class="plectr-live__chart" aria-hidden="true"><i style="transform: scaleX({progress})"></i></div>

  <div class="plectr-live__keys" aria-label={t("plectr.howTo.keys")}>
    {#each LANES as lane, i (lane.name)}
      <kbd style="--lane-color: {lane.color}">{keyName(keys[i] ?? "")}</kbd>
    {/each}
  </div>
  <p class="plectr-live__hint">{t("plectr.howTo.short")}</p>
</section>
