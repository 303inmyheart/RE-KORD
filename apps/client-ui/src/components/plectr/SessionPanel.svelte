<script lang="ts">
  /** Right panel (desktop) / info sheet (tablet): live run stats, combo meter, the keys. */
  import { fmtNumber, t } from "../../lib/i18n.svelte";
  import { LANES } from "../../lib/plectr/config";
  import { comboMultiplier } from "../../lib/plectr/engine";
  import type { LiveStats } from "./GameStage.svelte";

  let { stats, keys }: { stats: LiveStats | null; keys: string[] } = $props();

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

<section class="plectr-panel plectr-panel--session" aria-label={t("plectr.panel.session")}>
  <h3 class="plectr-panel__heading">{t("plectr.panel.session")}</h3>
  <div class="plectr-live__acc">
    <strong>{s.judged ? pct(s.accuracy) : "—"}</strong>
    <span>{t("plectr.results.accuracy")}</span>
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
  <div class="plectr-live__keys" aria-label={t("plectr.howTo.keys")}>
    {#each LANES as lane, i (lane.name)}
      <kbd style="--lane-color: {lane.color}">{keyName(keys[i] ?? "")}</kbd>
    {/each}
  </div>
  <p class="plectr-live__hint">{t("plectr.howTo.short")}</p>
</section>
