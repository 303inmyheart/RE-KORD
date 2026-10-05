/**
 * Share card: a 1080×1350 PNG of a run (cover wash, grade, score, accuracy,
 * combo, FC / AP), saved through a download link.
 */
import { fmtNumber, t } from "../i18n.svelte";
import { JUDGE_COLORS, STAGE_BG } from "./config";
import type { DifficultyId, GameResult } from "./types";

export type ShareCardData = {
  title: string;
  artist: string;
  difficulty: DifficultyId;
  result: GameResult;
  fc: boolean;
  ap: boolean;
  newRecord: boolean;
};

const W = 1080;
const H = 1350;

const GRADE_COLORS: Record<string, string> = {
  S: "#fde047",
  A: "#46e7ff",
  B: "#4ade80",
  C: "#fbbf24",
  D: "#f87171",
  F: "#f87171",
};

function fitText(ctx: CanvasRenderingContext2D, text: string, maxWidth: number): string {
  if (ctx.measureText(text).width <= maxWidth) return text;
  let s = text;
  while (s.length > 1 && ctx.measureText(`${s}…`).width > maxWidth) s = s.slice(0, -1);
  return `${s}…`;
}

export function renderShareCard(data: ShareCardData, art: CanvasImageSource | null): HTMLCanvasElement {
  const c = document.createElement("canvas");
  c.width = W;
  c.height = H;
  const ctx = c.getContext("2d")!;
  ctx.fillStyle = STAGE_BG;
  ctx.fillRect(0, 0, W, H);
  if (art) {
    ctx.globalAlpha = 0.55;
    ctx.drawImage(art, -H * 0.1, -H * 0.1, H * 1.2, H * 1.2);
    ctx.globalAlpha = 1;
  }
  const veil = ctx.createLinearGradient(0, 0, 0, H);
  veil.addColorStop(0, "rgba(6, 8, 15, 0.35)");
  veil.addColorStop(1, "rgba(6, 8, 15, 0.92)");
  ctx.fillStyle = veil;
  ctx.fillRect(0, 0, W, H);

  const font = (weight: number, size: number, italic = false) =>
    `${italic ? "italic " : ""}${weight} ${size}px Inter, system-ui, sans-serif`;
  ctx.textBaseline = "alphabetic";
  ctx.fillStyle = "rgba(255,255,255,0.75)";
  ctx.font = font(800, 40);
  ctx.fillText("PLECTR · RE-KORD", 80, 120);

  ctx.fillStyle = "#ffffff";
  ctx.font = font(800, 72);
  ctx.fillText(fitText(ctx, data.title, W - 160), 80, 260);
  ctx.fillStyle = "rgba(255,255,255,0.7)";
  ctx.font = font(600, 44);
  ctx.fillText(fitText(ctx, data.artist, W - 160), 80, 330);
  ctx.fillText(t(`plectr.diff.${data.difficulty}`), 80, 395);

  const grade = data.result.grade;
  ctx.fillStyle = GRADE_COLORS[grade] ?? "#ffffff";
  ctx.font = font(900, 420, true);
  ctx.fillText(grade, 70, 860);

  ctx.fillStyle = "#ffffff";
  ctx.font = font(800, 110);
  ctx.textAlign = "right";
  ctx.fillText(fmtNumber(data.result.score), W - 80, 700);
  ctx.font = font(600, 44);
  ctx.fillStyle = "rgba(255,255,255,0.75)";
  ctx.fillText(t("plectr.hudScore"), W - 80, 760);
  if (data.ap || data.fc || data.newRecord) {
    ctx.fillStyle = data.ap ? JUDGE_COLORS.perfect : JUDGE_COLORS.good;
    ctx.font = font(900, 54, true);
    const label = data.ap
      ? t("plectr.results.allPerfect")
      : data.fc
        ? t("plectr.results.fullCombo")
        : t("plectr.results.newRecord");
    ctx.fillText(label, W - 80, 850);
  }
  ctx.textAlign = "left";

  const acc = `${Math.round(data.result.accuracy * 1000) / 10}%`;
  const rows: [string, string][] = [
    [t("plectr.results.accuracy"), acc],
    [t("plectr.results.maxCombo"), `×${data.result.maxCombo}`],
    [t("plectr.results.miss"), String(data.result.misses)],
  ];
  rows.forEach(([label, value], i) => {
    const x = 80 + i * 320;
    ctx.fillStyle = "rgba(255,255,255,0.65)";
    ctx.font = font(600, 36);
    ctx.fillText(label, x, 1080);
    ctx.fillStyle = "#ffffff";
    ctx.font = font(800, 72);
    ctx.fillText(value, x, 1165);
  });
  ctx.fillStyle = "rgba(255,255,255,0.45)";
  ctx.font = font(600, 32);
  ctx.fillText(new Date().toLocaleDateString(), 80, 1270);
  return c;
}

export async function downloadShareCard(data: ShareCardData, art: CanvasImageSource | null): Promise<void> {
  const canvas = renderShareCard(data, art);
  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
  if (!blob) return;
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `plectr-${data.title.replace(/[^\p{L}\p{N}]+/gu, "-").slice(0, 60) || "run"}.png`;
  document.body.appendChild(a);
  a.click();
  a.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 5000);
}
