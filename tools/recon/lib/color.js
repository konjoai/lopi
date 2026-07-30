// WCAG contrast + colour-normalisation helpers for Census A.
'use strict';

function parseColor(str) {
  if (!str) return null;
  const m = str.match(/rgba?\(([^)]+)\)/);
  if (!m) return null;
  const parts = m[1].split(',').map((s) => parseFloat(s.trim()));
  const [r, g, b, a = 1] = parts;
  if ([r, g, b].some((n) => Number.isNaN(n))) return null;
  return { r, g, b, a };
}

function toHex({ r, g, b }) {
  const h = (n) => Math.round(n).toString(16).padStart(2, '0');
  return `#${h(r)}${h(g)}${h(b)}`.toLowerCase();
}

function relLuminance({ r, g, b }) {
  const chan = (c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  };
  return 0.2126 * chan(r) + 0.7152 * chan(g) + 0.0722 * chan(b);
}

/** WCAG contrast ratio between two {r,g,b} colours, 1..21. */
function contrastRatio(a, b) {
  const l1 = relLuminance(a) + 0.05;
  const l2 = relLuminance(b) + 0.05;
  return l1 > l2 ? l1 / l2 : l2 / l1;
}

/** Simple perceptual-ish distance (Euclidean in sRGB) — good enough to
 *  cluster near-duplicate greys/accents without pulling in a Lab-space
 *  colour-math dependency for a one-off recon script. */
function rgbDistance(a, b) {
  return Math.sqrt((a.r - b.r) ** 2 + (a.g - b.g) ** 2 + (a.b - b.b) ** 2);
}

/** HSL saturation (0..1) from an {r,g,b} triple, for the "simultaneous
 *  saturation" count. */
function saturation({ r, g, b }) {
  const [rn, gn, bn] = [r / 255, g / 255, b / 255];
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const l = (max + min) / 2;
  if (max === min) return 0;
  const d = max - min;
  return l > 0.5 ? d / (2 - max - min) : d / (max + min);
}

module.exports = { parseColor, toHex, contrastRatio, rgbDistance, saturation, relLuminance };
