<script lang="ts">
  /** Plectr career: tracks, counted runs, full runs, notes hit, grade distribution, FC / AP, XP. */
  import { fmtNumber, t } from "../../lib/i18n.svelte";
  import { GRADES, type PlectrCareer } from "../../lib/plectr/records";

  let { career, compact = false }: { career: PlectrCareer; compact?: boolean } = $props();

  const gradeTotal = $derived(Math.max(1, GRADES.reduce((n, g) => n + career.grades[g], 0)));
</script>

<section class="plectr-career" class:is-compact={compact} aria-label={t("plectr.career.title")}>
  <header class="plectr-career__head">
    <h3>{t("plectr.career.title")}</h3>
    <span class="plectr-career__xp">{t("plectr.career.xp", { n: fmtNumber(career.xp) })}</span>
  </header>
  <dl class="plectr-career__facts">
    <div><dt>{t("plectr.career.tracks")}</dt><dd>{fmtNumber(career.tracksPlayed)}</dd></div>
    <div><dt>{t("plectr.career.runs")}</dt><dd>{fmtNumber(career.runs)}</dd></div>
    <div><dt>{t("plectr.career.fullRuns")}</dt><dd>{fmtNumber(career.fullRuns)}</dd></div>
    <div><dt>{t("plectr.career.notes")}</dt><dd>{fmtNumber(career.notesHit)}</dd></div>
    {#if !compact}
      <div><dt>{t("plectr.fc")}</dt><dd>{fmtNumber(career.fullCombos)}</dd></div>
      <div><dt>{t("plectr.ap")}</dt><dd>{fmtNumber(career.allPerfects)}</dd></div>
    {/if}
  </dl>
  <div class="plectr-career__grades" aria-label={t("plectr.career.grades")}>
    {#each GRADES as g (g)}
      <div class="plectr-career__grade plectr-chip--{g.toLowerCase()}">
        <span class="plectr-career__bar"><i style="transform: scaleY({career.grades[g] / gradeTotal})"></i></span>
        <strong>{g}</strong>
        <small>{career.grades[g]}</small>
      </div>
    {/each}
  </div>
</section>
