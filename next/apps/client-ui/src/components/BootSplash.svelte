<script lang="ts">
  /**
   * Boot screen while the app looks for its hub. Usually gone in a few hundred
   * milliseconds, so nothing but the mark shows at first; the progress line
   * and a rotating tip fade in only if the wait gets long enough to read them
   * (parity with the 5.x RekordSplashLoader).
   */
  import { onMount } from "svelte";
  import { BrandLogo } from "@rekord/ui";
  import { t } from "../lib/i18n.svelte";

  const TIP_COUNT = 10;
  const TIP_ROTATE_MS = 4200;
  /** Below this a boot is "instant": no text flashes on screen. */
  const DETAILS_AFTER_MS = 700;

  let showDetails = $state(false);
  let tip = $state(Math.floor(Math.random() * TIP_COUNT));

  onMount(() => {
    const reveal = window.setTimeout(() => (showDetails = true), DETAILS_AFTER_MS);
    const rotate = window.setInterval(() => {
      const next = Math.floor(Math.random() * (TIP_COUNT - 1));
      tip = next >= tip ? next + 1 : next;
    }, TIP_ROTATE_MS);
    return () => {
      window.clearTimeout(reveal);
      window.clearInterval(rotate);
    };
  });
</script>

<div class="splash" role="status" aria-live="polite" aria-label={t("ui.splash.aria")}>
  <div class="splash__mark">
    <span class="splash__logo"><BrandLogo size="lg" /></span>
    <span class="splash__word">RE-KORD</span>
  </div>

  <div class="splash__details" class:is-on={showDetails} aria-hidden={!showDetails}>
    <div class="splash__track" aria-hidden="true"><span class="splash__bar"></span></div>
    <p class="splash__status">{t("ui.splash.status")}</p>
    {#key tip}
      <p class="splash__tip">{t(`ui.splash.tip${tip + 1}`)}</p>
    {/key}
  </div>
</div>

<style>
  .splash {
    position: relative;
    display: grid;
    place-items: center;
    align-content: center;
    gap: 2rem;
    min-height: var(--rk-app-vh);
    padding: 2rem 1.25rem calc(2rem + env(safe-area-inset-bottom, 0px));
    background:
      radial-gradient(
        ellipse 60% 45% at 50% 42%,
        color-mix(in srgb, var(--rk-accent) 10%, transparent),
        transparent 70%
      ),
      radial-gradient(
        ellipse 50% 40% at 55% 60%,
        color-mix(in srgb, var(--rk-accent-2) 8%, transparent),
        transparent 70%
      ),
      var(--rk-bg);
    text-align: center;
    overflow: hidden;
  }

  .splash__mark {
    display: grid;
    justify-items: center;
    gap: 0.85rem;
  }

  /* The logo is its own white rounded tile: no extra frame, just lift. */
  .splash__logo {
    display: block;
    border-radius: 18%;
    box-shadow:
      0 18px 44px color-mix(in srgb, black 45%, transparent),
      0 0 0 8px color-mix(in srgb, var(--rk-accent) 8%, transparent);
  }

  .splash__logo :global(.rk-logo) {
    width: 4.5rem;
    height: 4.5rem;
  }

  .splash__word {
    font-size: var(--rk-fs-5);
    font-weight: 800;
    letter-spacing: 0.18em;
    color: var(--rk-ink);
  }

  .splash__details {
    display: grid;
    justify-items: center;
    gap: 0.75rem;
    width: min(22rem, 100%);
    min-height: 6rem;
    opacity: 0;
    transition: opacity 0.4s ease;
  }

  .splash__details.is-on {
    opacity: 1;
  }

  .splash__track {
    position: relative;
    width: 9rem;
    height: 3px;
    border-radius: var(--rk-radius-round);
    background: color-mix(in srgb, var(--rk-ink) 10%, transparent);
    overflow: hidden;
  }

  .splash__bar {
    position: absolute;
    inset: 0;
    width: 40%;
    border-radius: inherit;
    background: linear-gradient(90deg, var(--rk-accent), var(--rk-accent-2));
    animation: splash-slide 1.3s cubic-bezier(0.65, 0, 0.35, 1) infinite;
  }

  /* Transform only: no layout, no repaint of the page behind it. */
  @keyframes splash-slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(250%);
    }
  }

  .splash__status {
    margin: 0;
    font-size: var(--rk-fs-2);
    font-weight: 600;
    color: var(--rk-muted-strong);
  }

  .splash__tip {
    margin: 0;
    max-width: 30ch;
    font-size: var(--rk-fs-2);
    line-height: var(--rk-lh);
    color: var(--rk-muted);
    text-wrap: balance;
    animation: splash-tip 0.45s ease both;
  }

  @keyframes splash-tip {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .splash__bar {
      animation: none;
      width: 100%;
      opacity: 0.6;
    }

    .splash__tip {
      animation: none;
    }
  }
</style>
