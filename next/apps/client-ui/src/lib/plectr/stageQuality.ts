/**
 * Light stage ("Palco leggero"): auto-on for software-composited engines
 * (WebKitGTK — Tauri on Linux), for weak devices and once a run measured
 * fewer than 50 fps on this device. The measurement is per device (local),
 * the user's choice (auto / on / off) is per account (synced settings).
 */
import { platformCaps } from "../platformCaps";
import { prefersReducedMotion } from "../visualizer/renderQuality";
import type { LightStageMode } from "./records";

const AUTO_KEY = "rekord.next.plectr.autoLight";
export const LOW_FPS_THRESHOLD = 50;

/** WebKitGTK / Tauri on Linux: every canvas frame goes through the CPU. */
export function isSlowStageEngine(): boolean {
  return platformCaps.webkitGtk;
}

/** Few cores / little memory. */
export function isWeakDevice(): boolean {
  if (typeof navigator === "undefined") return false;
  const nav = navigator as Navigator & { deviceMemory?: number };
  return (nav.hardwareConcurrency ?? 8) <= 4 || (nav.deviceMemory ?? 8) <= 3;
}

export function readMeasuredSlow(): boolean {
  try {
    return localStorage.getItem(AUTO_KEY) === "1";
  } catch {
    return false;
  }
}

export function writeMeasuredSlow(slow: boolean): void {
  try {
    if (slow) localStorage.setItem(AUTO_KEY, "1");
    else localStorage.removeItem(AUTO_KEY);
  } catch {
    /* private mode */
  }
}

/** What "auto" means on this device right now. */
export function autoLightStage(): boolean {
  return isSlowStageEngine() || isWeakDevice() || prefersReducedMotion() || readMeasuredSlow();
}

export function resolveLightStage(mode: LightStageMode): boolean {
  if (mode === "on") return true;
  if (mode === "off") return false;
  return autoLightStage();
}

/**
 * Frame-rate watch during play: after a warm-up, a window averaging under
 * the threshold reports `true` once (the caller switches to light stage).
 */
export class FpsWatch {
  private frames = 0;
  private windowStart = 0;
  private startedAt = 0;
  private done = false;

  constructor(
    private readonly warmupMs = 1500,
    private readonly windowMs = 4000,
  ) {}

  reset(now: number): void {
    this.frames = 0;
    this.windowStart = 0;
    this.startedAt = now;
  }

  /** Feed one frame; true when the device is too slow (reported once). */
  tick(now: number): boolean {
    if (this.done) return false;
    if (!this.startedAt) this.startedAt = now;
    if (now - this.startedAt < this.warmupMs) return false;
    if (!this.windowStart) {
      this.windowStart = now;
      this.frames = 0;
      return false;
    }
    this.frames += 1;
    const elapsed = now - this.windowStart;
    if (elapsed < this.windowMs) return false;
    const fps = (this.frames * 1000) / elapsed;
    this.windowStart = now;
    this.frames = 0;
    if (fps < LOW_FPS_THRESHOLD) {
      this.done = true;
      return true;
    }
    return false;
  }
}
