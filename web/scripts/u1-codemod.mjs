#!/usr/bin/env node
// Sprint U1 Step 3 — one-shot mechanical literal→token substitution.
// Exact-value substitution only: every replacement preserves the rendered
// pixel exactly (hex → var(--k-x) of an identical hex; rgba(r,g,b,a) →
// rgb(var(--k-x-rgb) / a) with 'a' preserved verbatim). Run once, then
// re-grep for stragglers — ambiguous multi-role triples (documented in
// LEDGER.md) are excluded here and handled by hand afterward.
'use strict';
import fs from 'node:fs';
import path from 'node:path';

const ROOT = path.join(import.meta.dirname, '..', 'src');
const EXEMPT = new Set([
  path.join(ROOT, 'lib', 'stores', 'phase-colors.ts'),
  path.join(ROOT, 'lib', 'forge', 'orbState.ts'),
  // Not itself browser-DOM-free, but asserts literal hex strings against the
  // exempted orbState.ts source — codemodding it desyncs the two. (Caught by
  // `npm run test` after the first codemod pass; reverted by hand.)
  path.join(ROOT, 'lib', 'forge', 'orbState.test.ts')
]);
const SKIP_BASENAMES = new Set(['tokens.css']);

// hex → token name (var(--k-...))
const HEX_MAP = {
  '#f5f5f5': '--k-text-primary',
  '#ff9500': '--k-chip-loop',
  '#ff0066': '--k-danger',
  '#00d4ff': '--k-chip-repo',
  '#b79bff': '--k-chip-model',
  '#ffcc00': '--k-chip-effort',
  '#00ff9d': '--k-preset-benchmark',
  '#00ffd4': '--k-chip-alias',
  '#ff9e12': '--k-ext-mark-logo',
  '#0a0d0f': '--k-surface-raised',
  '#ff4500': '--k-ext-ember',
  '#9d5cff': '--k-ext-violet-bright',
  '#7c3aed': '--k-ext-violet-testing',
  '#5ee6ff': '--k-ext-plasma',
  '#3be6c8': '--k-ext-mint',
  '#0b0e10': '--k-ext-black-fallback',
  '#231000': '--k-ext-bug-231000',
  '#0088aa': '--k-ext-ice-deep',
  '#ffb648': '--k-ext-flame-grad-a',
  '#b04a6a': '--k-ext-rose-muted',
  '#101013': '--k-ext-surface-card',
  '#0e1214': '--k-ext-surface-panel',
  '#0a0a0a': '--k-ext-surface-black',
  '#ffc670': '--k-ext-flame-grad-b',
  '#ffaacb': '--k-ext-stackcard-pink',
  '#ffa733': '--k-ext-flame-grad-c',
  '#ffa11a': '--k-ext-flame-grad-e',
  '#f85149': '--k-diff-del',
  '#f08600': '--k-ext-flame-grad-d',
  '#e6edf3': '--k-ext-code-text',
  '#b388ff': '--k-ext-violet-connbadge',
  '#66b3ff': '--k-ext-repo-light',
  '#3fb950': '--k-diff-add',
  '#1a1030': '--k-ext-dock-plum',
  '#0d1117': '--k-ext-code-bg',
  '#050505': '--k-surface-base',
  '#04141c': '--k-ext-sched-dark-teal',
  '#04120c': '--k-ext-evals-dark-green'
  // '#183,155,255' family intentionally excluded — dual role, handled by hand.
};

// rgb triple (normalized "r,g,b") → token name (var(--k-...-rgb))
// 183,155,255 intentionally excluded (dual role: chip-model vs border-interactive).
const RGB_MAP = {
  '255,255,255': '--k-wash-rgb',
  '245,245,245': '--k-text-primary-rgb',
  '255,149,0': '--k-chip-loop-rgb',
  '255,204,0': '--k-chip-effort-rgb',
  '0,212,255': '--k-chip-repo-rgb',
  '255,0,102': '--k-danger-rgb',
  '0,0,0': '--k-shadow-rgb',
  '0,255,212': '--k-chip-alias-rgb',
  '0,255,157': '--k-preset-benchmark-rgb',
  '59,230,200': '--k-ext-mint-rgb',
  '255,90,90': '--k-ext-red-banner-rgb',
  '157,92,255': '--k-ext-violet-bright-rgb',
  '124,58,237': '--k-ext-violet-testing-rgb',
  '120,90,200': '--k-ext-violet-dock-c-rgb',
  '102,179,255': '--k-ext-repo-light-rgb',
  '8,8,10': '--k-ext-surface-dropdown-rgb',
  '46,160,67': '--k-diff-add-bg-rgb',
  '255,69,0': '--k-ext-ember-rgb',
  '255,45,120': '--k-ext-rose-hover-rgb',
  '255,170,170': '--k-ext-red-banner-light-rgb',
  '255,120,0': '--k-ext-orange-warn-rgb',
  '255,110,150': '--k-ext-rose-soft-rgb',
  '248,81,73': '--k-diff-del-rgb',
  '179,136,255': '--k-ext-violet-connbadge-rgb',
  '150,255,210': '--k-ext-green-banner-light-rgb',
  '150,120,230': '--k-ext-violet-dock-a-rgb',
  '120,92,205': '--k-ext-violet-dock-b-rgb'
};

function walk(dir, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(p, out);
    else if (/\.(svelte|ts|css|js)$/.test(entry.name) && !SKIP_BASENAMES.has(entry.name)) out.push(p);
  }
  return out;
}

let filesChanged = 0;
let hexSubs = 0;
let rgbaSubs = 0;

for (const file of walk(ROOT)) {
  if (EXEMPT.has(file)) continue;
  let src = fs.readFileSync(file, 'utf8');
  const original = src;

  // hex literals (case-insensitive), longest-first doesn't matter since all are 6-digit.
  for (const [hex, token] of Object.entries(HEX_MAP)) {
    const re = new RegExp(hex.replace('#', '#'), 'gi');
    src = src.replace(re, (m) => {
      hexSubs++;
      return `var(${token})`;
    });
  }

  // rgba(r,g,b,a) and rgb(r,g,b) — flexible whitespace around commas.
  src = src.replace(
    /rgba?\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})\s*(?:,\s*([\d.]+)\s*)?\)/g,
    (whole, r, g, b, a) => {
      const key = `${r},${g},${b}`;
      const token = RGB_MAP[key];
      if (!token) return whole; // leave unmapped (ambiguous 183,155,255, or unseen) for manual pass
      rgbaSubs++;
      return a !== undefined ? `rgb(var(${token}) / ${a})` : `rgb(var(${token}))`;
    }
  );

  if (src !== original) {
    fs.writeFileSync(file, src);
    filesChanged++;
  }
}

console.log(`files changed: ${filesChanged}, hex substitutions: ${hexSubs}, rgba/rgb substitutions: ${rgbaSubs}`);
