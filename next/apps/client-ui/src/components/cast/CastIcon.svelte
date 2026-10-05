<script lang="ts">
  let {
    connected = false,
    connecting = false,
  }: { connected?: boolean; connecting?: boolean } = $props();
</script>

<svg class="cast-ic" class:is-pulsing={connecting} viewBox="0 0 24 24" aria-hidden="true">
  <path
    fill="none"
    stroke="currentColor"
    stroke-width="1.8"
    stroke-linecap="round"
    stroke-linejoin="round"
    d="M3 8V6a1 1 0 0 1 1-1h16a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1h-6M3 12a7 7 0 0 1 7 7M3 15.5A3.5 3.5 0 0 1 6.5 19"
  />
  <circle cx="3.6" cy="18.4" r="1" fill="currentColor" />
  {#if connected}
    <path fill="currentColor" opacity="0.55" d="M7 8h11v8h-3.2A10 10 0 0 0 7 8.6z" />
  {/if}
</svg>

<style>
  .cast-ic {
    width: 1.25rem;
    height: 1.25rem;
    display: block;
  }

  .cast-ic.is-pulsing {
    animation: cast-pulse 1.1s ease-in-out infinite alternate;
  }

  @keyframes cast-pulse {
    from {
      opacity: 0.45;
    }
    to {
      opacity: 1;
    }
  }
</style>
