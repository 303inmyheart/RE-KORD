<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  /**
   * Lyrics glyph of a track: synced (LRC) or plain text. Nothing at all when the
   * track has no lyrics — a near-invisible ghost icon read as noise.
   */
  let {
    kind = "off" as "off" | "plain" | "lrc",
    class: className = "",
  }: {
    kind?: "off" | "plain" | "lrc";
    class?: string;
  } = $props();
</script>

{#if kind !== "off"}
  <span
    class="track-row__lyrics-inline {className}"
    class:is-plain={kind === "plain"}
    class:is-lrc={kind === "lrc"}
    role="img"
    title={kind === "lrc" ? t("lyricsIcon.lrc") : t("lyricsIcon.plain")}
    aria-label={kind === "lrc" ? t("lyricsIcon.lrc") : t("lyricsIcon.plain")}
  >
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      {#if kind === "lrc"}
        <path d="M12 3v10.55A4 4 0 1014 17V7h4V3h-6zM4 19h10v2H4v-2zm0-4h7v2H4v-2z" />
      {:else}
        <path
          d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8l-6-6zm-1 13H9v-2h4v2zm0-4H9V9h4v2zm-1-5V3.5L18.5 9H12z"
        />
      {/if}
    </svg>
  </span>
{/if}
