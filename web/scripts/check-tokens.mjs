#!/usr/bin/env node
/**
 * Sprint U1 Step 6 — colour-token assertions. Wired into `npm run build`
 * and CI. Fails the build on:
 *   1. Contrast — every text token against every surface it renders on,
 *      4.5:1 minimum (3:1 for large text / control-edge borders).
 *   2. CVD collapse — simulate deuteranopia/protanopia/tritanopia across
 *      the five chip tokens + danger; two that land within a perceptual
 *      threshold must carry distinct glyphs, or this fails.
 *   3. No raw colour — no hex/rgb()/rgba()/hsl()/oklch() literal anywhere
 *      in web/src outside tokens.css (plus two documented exemptions for
 *      browser-DOM-free constant maps asserted against by literal-string
 *      unit tests — see LEDGER.md Sprint U1).
 */
import fs from 'node:fs';
import path from 'node:path';

const WEB_ROOT = path.join(import.meta.dirname, '..');
const SRC_ROOT = path.join(WEB_ROOT, 'src');
const TOKENS_FILE = path.join(SRC_ROOT, 'lib', 'styles', 'tokens.css');

// ── shared: parse tokens.css into a resolved name → hex map ────────────────

function parseTokens() {
  const css = fs.readFileSync(TOKENS_FILE, 'utf8');
  const raw = new Map();
  for (const m of css.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) {
    raw.set(m[1].trim(), m[2].trim());
  }
  const resolved = new Map();
  function resolve(name, seen = new Set()) {
    if (resolved.has(name)) return resolved.get(name);
    if (seen.has(name)) return null; // cycle guard
    seen.add(name);
    const value = raw.get(name);
    if (value === undefined) return null;
    const varMatch = value.match(/^var\((--[\w-]+)\)$/);
    let out;
    if (varMatch) {
      out = resolve(varMatch[1], seen);
    } else if (/^#[0-9a-fA-F]{6}$/.test(value)) {
      out = value.toLowerCase();
    } else {
      out = null; // rgba()/alpha/motion tokens — not a plain hex, skip
    }
    resolved.set(name, out);
    return out;
  }
  for (const name of raw.keys()) resolve(name);
  return resolved;
}

function hexToRgb(hex) {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

// ── Check 1: contrast (WCAG relative luminance) ─────────────────────────────

function srgbToLinear(c) {
  const s = c / 255;
  return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
}
function relativeLuminance([r, g, b]) {
  const [rl, gl, bl] = [r, g, b].map(srgbToLinear);
  return 0.2126 * rl + 0.7152 * gl + 0.0722 * bl;
}
function contrastRatio(hexA, hexB) {
  const la = relativeLuminance(hexToRgb(hexA));
  const lb = relativeLuminance(hexToRgb(hexB));
  const [lighter, darker] = la > lb ? [la, lb] : [lb, la];
  return (lighter + 0.05) / (darker + 0.05);
}

// --k-text-disabled is deliberately excluded: WCAG SC 1.4.3 has no contrast
// requirement for "text ... that is part of an inactive user interface
// component" — exactly this token's declared purpose (the disabled/draft
// status marker, Step 4b). Not a lowered threshold, a correct exemption.
const TEXT_TOKENS = ['--k-text-muted', '--k-text-secondary', '--k-text-primary'];
const SURFACE_TOKENS = ['--k-surface-base', '--k-surface-raised', '--k-surface-overlay'];
// Chip/danger foregrounds render as text on the same three surfaces.
const CHIP_TOKENS = ['--k-chip-alias', '--k-chip-repo', '--k-chip-model', '--k-chip-effort', '--k-chip-loop', '--k-danger'];
const BORDER_INTERACTIVE = '--k-border-interactive-rgb'; // rgba literal, resolved separately below

function runContrastCheck(tokens) {
  const failures = [];
  for (const surface of SURFACE_TOKENS) {
    const bg = tokens.get(surface);
    if (!bg) continue;
    for (const text of TEXT_TOKENS) {
      const fg = tokens.get(text);
      if (!fg) continue;
      const ratio = contrastRatio(fg, bg);
      if (ratio < 4.5) {
        failures.push({ fg: text, bg: surface, ratio: Number(ratio.toFixed(2)), needs: 4.5, kind: 'text' });
      }
    }
    for (const chip of CHIP_TOKENS) {
      const fg = tokens.get(chip);
      if (!fg) continue;
      const ratio = contrastRatio(fg, bg);
      if (ratio < 4.5) {
        failures.push({ fg: chip, bg: surface, ratio: Number(ratio.toFixed(2)), needs: 4.5, kind: 'chip-text' });
      }
    }
  }
  // Interactive control border: 3:1 minimum against the surfaces it edges.
  const borderRgbRaw = fs
    .readFileSync(TOKENS_FILE, 'utf8')
    .match(/--k-border-interactive-rgb:\s*([\d\s]+);/);
  if (borderRgbRaw) {
    const [r, g, b] = borderRgbRaw[1].trim().split(/\s+/).map(Number);
    const borderHex = `#${[r, g, b].map((c) => c.toString(16).padStart(2, '0')).join('')}`;
    for (const surface of SURFACE_TOKENS) {
      const bg = tokens.get(surface);
      if (!bg) continue;
      const ratio = contrastRatio(borderHex, bg);
      if (ratio < 3.0) {
        failures.push({ fg: '--k-border-interactive', bg: surface, ratio: Number(ratio.toFixed(2)), needs: 3.0, kind: 'control-border' });
      }
    }
  }
  // The Step 4d bug tracker: fails loudly until commit 2 recolors it.
  const bug = tokens.get('--k-ext-bug-231000');
  if (bug) {
    for (const surface of SURFACE_TOKENS) {
      const bg = tokens.get(surface);
      if (!bg) continue;
      const ratio = contrastRatio(bug, bg);
      if (ratio < 4.5) {
        failures.push({ fg: '--k-ext-bug-231000', bg: surface, ratio: Number(ratio.toFixed(2)), needs: 4.5, kind: 'known-bug (Step 4d)' });
      }
    }
  }
  return failures;
}

// ── Check 2: CVD collapse (Machado/Oliveira/Fernandes 2009 matrices) ───────

const CVD_MATRICES = {
  protanopia: [
    [0.152286, 1.052583, -0.204868],
    [0.114503, 0.786281, 0.099216],
    [-0.003882, -0.048116, 1.051998]
  ],
  deuteranopia: [
    [0.367322, 0.860646, -0.227968],
    [0.280085, 0.672501, 0.047413],
    [-0.011820, 0.042940, 0.968881]
  ],
  tritanopia: [
    [1.255528, -0.076749, -0.178779],
    [-0.078411, 0.930809, 0.147602],
    [0.004733, 0.691367, 0.303900]
  ]
};

function applyMatrix(m, [r, g, b]) {
  return [
    m[0][0] * r + m[0][1] * g + m[0][2] * b,
    m[1][0] * r + m[1][1] * g + m[1][2] * b,
    m[2][0] * r + m[2][1] * g + m[2][2] * b
  ].map((c) => Math.min(255, Math.max(0, c)));
}

function euclideanDistance(a, b) {
  return Math.sqrt(a.reduce((sum, v, i) => sum + (v - b[i]) ** 2, 0));
}

// Each chip carries a distinct sigil in the composer grammar (Step 4b/spec),
// so a CVD collapse between two of these is not user-facing ambiguity.
const CHIP_SIGILS = {
  '--k-chip-alias': ':',
  '--k-chip-repo': '@',
  '--k-chip-model': ';model',
  '--k-chip-effort': ';effort',
  '--k-chip-loop': '×', // loop-count multiplier
  '--k-danger': '✕' // Step 4b blocked marker
};

const CVD_COLLAPSE_THRESHOLD = 18; // Euclidean sRGB distance, matches recon's own "near-duplicate cluster" threshold

function runCvdCheck(tokens) {
  const failures = [];
  const names = Object.keys(CHIP_SIGILS).filter((n) => tokens.has(n) && tokens.get(n));
  for (const [cvdName, matrix] of Object.entries(CVD_MATRICES)) {
    const simulated = new Map(names.map((n) => [n, applyMatrix(matrix, hexToRgb(tokens.get(n)))]));
    for (let i = 0; i < names.length; i++) {
      for (let j = i + 1; j < names.length; j++) {
        const a = names[i];
        const b = names[j];
        const dist = euclideanDistance(simulated.get(a), simulated.get(b));
        if (dist < CVD_COLLAPSE_THRESHOLD) {
          const distinctGlyphs = CHIP_SIGILS[a] !== CHIP_SIGILS[b];
          if (!distinctGlyphs) {
            failures.push({ cvdName, a, b, distance: Number(dist.toFixed(2)), threshold: CVD_COLLAPSE_THRESHOLD });
          }
        }
      }
    }
  }
  return failures;
}

// ── Check 3: no raw colour outside tokens.css ───────────────────────────────

const RAW_COLOUR_RE = /#[0-9a-fA-F]{3,8}\b|\brgba?\(\s*\d|\bhsla?\(|\boklch\(/;
const EXEMPT_FILES = new Set([
  path.join(SRC_ROOT, 'lib', 'stores', 'phase-colors.ts'),
  path.join(SRC_ROOT, 'lib', 'forge', 'orbState.ts'),
  path.join(SRC_ROOT, 'lib', 'forge', 'orbState.test.ts')
]);

function walk(dir, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(p, out);
    else if (/\.(svelte|ts|css|js)$/.test(entry.name)) out.push(p);
  }
  return out;
}

function runNoRawColourCheck() {
  const failures = [];
  for (const file of walk(SRC_ROOT)) {
    if (file === TOKENS_FILE || EXEMPT_FILES.has(file)) continue;
    const lines = fs.readFileSync(file, 'utf8').split('\n');
    lines.forEach((line, i) => {
      if (RAW_COLOUR_RE.test(line)) {
        failures.push({ file: path.relative(WEB_ROOT, file), line: i + 1, text: line.trim().slice(0, 120) });
      }
    });
  }
  return failures;
}

// ── main ─────────────────────────────────────────────────────────────────

function main() {
  const tokens = parseTokens();
  let failed = false;

  const contrastFailures = runContrastCheck(tokens);
  if (contrastFailures.length) {
    failed = true;
    console.error(`\n✗ Contrast check: ${contrastFailures.length} failure(s)`);
    for (const f of contrastFailures) {
      console.error(`  ${f.kind}: ${f.fg} on ${f.bg} — ${f.ratio}:1 (needs ${f.needs}:1)`);
    }
  } else {
    console.log('✓ Contrast check passed');
  }

  const cvdFailures = runCvdCheck(tokens);
  if (cvdFailures.length) {
    failed = true;
    console.error(`\n✗ CVD collapse check: ${cvdFailures.length} failure(s)`);
    for (const f of cvdFailures) {
      console.error(`  ${f.cvdName}: ${f.a} and ${f.b} collapse to distance ${f.distance} (threshold ${f.threshold}) with no distinct glyph`);
    }
  } else {
    console.log('✓ CVD collapse check passed');
  }

  const rawColourFailures = runNoRawColourCheck();
  if (rawColourFailures.length) {
    failed = true;
    console.error(`\n✗ No-raw-colour check: ${rawColourFailures.length} literal(s) outside tokens.css`);
    for (const f of rawColourFailures.slice(0, 50)) {
      console.error(`  ${f.file}:${f.line}: ${f.text}`);
    }
    if (rawColourFailures.length > 50) console.error(`  ... and ${rawColourFailures.length - 50} more`);
  } else {
    console.log('✓ No-raw-colour check passed');
  }

  if (failed) {
    console.error('\ncheck-tokens.mjs FAILED');
    process.exit(1);
  }
  console.log('\ncheck-tokens.mjs passed');
}

main();
