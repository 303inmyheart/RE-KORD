/**
 * Custom theme colours: palette extraction from image pixels, the text colour
 * (automatic or chosen) and its contrast check.
 *
 * Si lancia con `pnpm test` (node --experimental-strip-types).
 */
import assert from "node:assert/strict";
import { describe, test } from "node:test";
import {
  contrastRatio,
  extractThemeColorsFromPixels,
} from "./extractThemeColorsFromImage.ts";
import {
  autoInk,
  contrastHex,
  customTextContrast,
  customTextTokens,
  parseHex,
  pickReadableText,
  WCAG_AA_TEXT,
} from "./themeText.ts";
import { normalizeCustomTheme } from "./themeCatalog.ts";

const HEX = /^#[0-9a-f]{6}$/;

function fill(n, rgb) {
  return Array.from({ length: n }, () => rgb);
}

function hueOf({ r, g, b }) {
  const rn = r / 255;
  const gn = g / 255;
  const bn = b / 255;
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const d = max - min;
  if (d === 0) return 0;
  let h;
  if (max === rn) h = 60 * (((gn - bn) / d) % 6);
  else if (max === gn) h = 60 * ((bn - rn) / d + 2);
  else h = 60 * ((rn - gn) / d + 4);
  return (h + 360) % 360;
}

function hueDist(a, b) {
  const d = Math.abs(a - b) % 360;
  return d > 180 ? 360 - d : d;
}

describe("extractThemeColorsFromPixels", () => {
  test("too few pixels gives null", () => {
    assert.equal(extractThemeColorsFromPixels(fill(8, { r: 1, g: 2, b: 3 })), null);
  });

  test("returns five valid hex colours for a solid field", () => {
    const c = extractThemeColorsFromPixels(fill(1024, { r: 20, g: 40, b: 80 }));
    assert.ok(c);
    for (const key of ["bg", "section", "accent", "accent2", "text"]) {
      assert.match(c[key], HEX, key);
    }
  });

  test("dark image: dark background, slightly lighter sections", () => {
    const c = extractThemeColorsFromPixels(fill(2304, { r: 18, g: 22, b: 34 }));
    const bg = parseHex(c.bg);
    const section = parseHex(c.section);
    assert.ok(Math.max(bg.r, bg.g, bg.b) < 80);
    assert.ok(section.r + section.g + section.b > bg.r + bg.g + bg.b);
  });

  test("light image: light background and dark text", () => {
    const c = extractThemeColorsFromPixels(fill(2304, { r: 235, g: 238, b: 244 }));
    const bg = parseHex(c.bg);
    assert.ok(Math.min(bg.r, bg.g, bg.b) > 180);
    assert.ok(contrastHex(c.text, "#000000") < 3, `text ${c.text} should be dark`);
  });

  test("accent follows the dominant saturated hue", () => {
    const pixels = [
      ...fill(1500, { r: 14, g: 16, b: 22 }),
      ...fill(500, { r: 220, g: 40, b: 40 }),
    ];
    const c = extractThemeColorsFromPixels(pixels);
    assert.ok(hueDist(hueOf(parseHex(c.accent)), 0) < 20, `accent ${c.accent} is red`);
  });

  test("accent and text are readable on the sections (AA)", () => {
    const samples = [
      { r: 18, g: 22, b: 34 },
      { r: 235, g: 238, b: 244 },
      { r: 120, g: 110, b: 100 },
      { r: 40, g: 160, b: 90 },
    ];
    for (const rgb of samples) {
      const pixels = [...fill(1600, rgb), ...fill(400, { r: 240, g: 180, b: 30 })];
      const c = extractThemeColorsFromPixels(pixels);
      assert.ok(
        contrastRatio(parseHex(c.accent), parseHex(c.section)) >= 4.5,
        `accent ${c.accent} on ${c.section}`,
      );
      assert.ok(
        customTextContrast({ bg: c.bg, section: c.section, text: c.text }) >= WCAG_AA_TEXT,
        `text ${c.text} on ${c.section} / ${c.bg}`,
      );
    }
  });

  test("a mid-grey image still gets AA text", () => {
    const c = extractThemeColorsFromPixels(fill(2048, { r: 128, g: 128, b: 128 }));
    assert.ok(contrastHex(c.text, c.section) >= WCAG_AA_TEXT);
  });
});

describe("text colour", () => {
  test("automatic tokens match the previous theme ink", () => {
    const dark = customTextTokens({ section: "#121f31" });
    assert.equal(dark.ink, autoInk("#121f31"));
    const light = customTextTokens({ section: "#e4e9f2" });
    assert.equal(light.ink, autoInk("#e4e9f2"));
    assert.ok(contrastHex(dark.ink, "#121f31") > 10);
    assert.ok(contrastHex(light.ink, "#e4e9f2") > 7);
  });

  test("a chosen colour drives ink and the derived muted inks", () => {
    const tokens = customTextTokens({ section: "#121f31", text: "#ffd166" });
    assert.equal(tokens.ink, "#ffd166");
    for (const key of ["muted", "mutedStrong"]) {
      assert.match(tokens[key], HEX);
      // between the section and the ink: lower contrast than the ink itself
      assert.ok(contrastHex(tokens[key], "#121f31") < contrastHex("#ffd166", "#121f31"));
      assert.ok(contrastHex(tokens[key], "#121f31") > 1.5);
    }
    // muted-strong is closer to the ink than muted
    assert.ok(
      contrastHex(tokens.mutedStrong, "#121f31") > contrastHex(tokens.muted, "#121f31"),
    );
  });

  test("contrast check flags dark text on a dark theme", () => {
    const theme = { bg: "#08111d", section: "#121f31", text: "#333a44" };
    assert.ok(customTextContrast(theme) < WCAG_AA_TEXT);
    assert.ok(customTextContrast({ ...theme, text: "#f0f0f0" }) >= WCAG_AA_TEXT);
  });

  test("pickReadableText reaches AA on hard mid-tones", () => {
    for (const section of ["#777777", "#3b82f6", "#c08040", "#5a5a5a", "#a0a0a0"]) {
      const text = pickReadableText(section, section);
      assert.ok(contrastHex(text, section) >= WCAG_AA_TEXT, `${text} on ${section}`);
    }
  });

  test("normalizeCustomTheme keeps a valid text colour and drops a bad one", () => {
    assert.equal(normalizeCustomTheme({ text: "#ABC" }).text, "#aabbcc");
    assert.equal("text" in normalizeCustomTheme({ text: "nope" }), false);
    assert.equal("text" in normalizeCustomTheme({ text: null }), false);
    assert.equal("text" in normalizeCustomTheme({}), false);
  });
});
