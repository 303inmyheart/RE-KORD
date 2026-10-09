<script lang="ts">
  /** E / N / H grade chips of a track (one per difficulty), FC / AP marks; nothing until it has a record. */
  import { t } from "../../lib/i18n.svelte";
  import { DIFFICULTY_IDS } from "../../lib/plectr/config";
  import type { DifficultyBests } from "../../lib/plectr/records";

  let { bests }: { bests: DifficultyBests | null | undefined } = $props();
</script>

{#if bests && Object.keys(bests).length}
<span class="plectr-chips">
  {#each DIFFICULTY_IDS as id (id)}
    {@const best = bests?.[id]}
    <span
      class="plectr-chip plectr-chip--{best ? best.grade.toLowerCase() : 'none'}"
      title={best
        ? t("plectr.chipTitle", { diff: t(`plectr.diff.${id}`), grade: best.grade })
        : t("plectr.chipEmpty", { diff: t(`plectr.diff.${id}`) })}
    >
      <span class="plectr-chip__diff">{t(`plectr.diffShort.${id}`)}</span>
      <strong>{best ? best.grade : "–"}</strong>
      {#if best?.ap}<em class="plectr-chip__mark">{t("plectr.ap")}</em>{:else if best?.fc}<em class="plectr-chip__mark">{t("plectr.fc")}</em>{/if}
    </span>
  {/each}
</span>
{/if}
