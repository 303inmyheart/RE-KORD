<script lang="ts">
  import Metric from "./Metric.svelte";

  /** @deprecated Use `Metric` (same look, more options). Kept for older callers. */
  let {
    label,
    value,
  }: {
    label: string;
    value: string | number;
  } = $props();
</script>

<Metric {label} {value} />
