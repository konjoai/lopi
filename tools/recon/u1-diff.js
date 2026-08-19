// Sprint U1 — pixel-diff two recon/u1/<name>/ capture sets, image-by-image.
// Usage: node u1-diff.js <before-name> <after-name> [--out summary.json]
'use strict';

const fs = require('fs');
const path = require('path');
const { PNG } = require('pngjs');
const pixelmatch = require('pixelmatch');

const [, , beforeName, afterName] = process.argv;
if (!beforeName || !afterName) {
  console.error('usage: node u1-diff.js <before-name> <after-name>');
  process.exit(1);
}

const ROOT = path.join(__dirname, '..', '..', 'recon', 'u1');
const BEFORE = path.join(ROOT, beforeName);
const AFTER = path.join(ROOT, afterName);
const DIFF_DIR = path.join(ROOT, `${afterName}-diff`);

function listPngs(dir) {
  const out = [];
  for (const sub of ['pages', 'components']) {
    const d = path.join(dir, sub);
    if (!fs.existsSync(d)) continue;
    for (const f of fs.readdirSync(d)) {
      if (f.endsWith('.png')) out.push(path.join(sub, f));
    }
  }
  return out;
}

function readPng(p) {
  return PNG.sync.read(fs.readFileSync(p));
}

function main() {
  fs.mkdirSync(path.join(DIFF_DIR, 'pages'), { recursive: true });
  fs.mkdirSync(path.join(DIFF_DIR, 'components'), { recursive: true });

  const beforeFiles = new Set(listPngs(BEFORE));
  const afterFiles = new Set(listPngs(AFTER));
  const allFiles = new Set([...beforeFiles, ...afterFiles]);

  const results = [];
  for (const rel of [...allFiles].sort()) {
    const beforePath = path.join(BEFORE, rel);
    const afterPath = path.join(AFTER, rel);
    if (!beforeFiles.has(rel)) {
      results.push({ file: rel, status: 'missing-in-before' });
      continue;
    }
    if (!afterFiles.has(rel)) {
      results.push({ file: rel, status: 'missing-in-after' });
      continue;
    }
    const img1 = readPng(beforePath);
    const img2 = readPng(afterPath);
    if (img1.width !== img2.width || img1.height !== img2.height) {
      results.push({
        file: rel,
        status: 'size-mismatch',
        before: `${img1.width}x${img1.height}`,
        after: `${img2.width}x${img2.height}`
      });
      continue;
    }
    const { width, height } = img1;
    const diff = new PNG({ width, height });
    const diffPixels = pixelmatch(img1.data, img2.data, diff.data, width, height, { threshold: 0.1 });
    const totalPixels = width * height;
    const pct = (diffPixels / totalPixels) * 100;
    if (diffPixels > 0) {
      const outFile = path.join(DIFF_DIR, rel);
      fs.mkdirSync(path.dirname(outFile), { recursive: true });
      fs.writeFileSync(outFile, PNG.sync.write(diff));
    }
    results.push({
      file: rel,
      status: diffPixels === 0 ? 'identical' : 'diff',
      diffPixels,
      totalPixels,
      diffPct: Number(pct.toFixed(4))
    });
  }

  const summaryPath = path.join(ROOT, `${afterName}-vs-${beforeName}.json`);
  fs.writeFileSync(summaryPath, JSON.stringify(results, null, 2));

  const nonIdentical = results.filter((r) => r.status !== 'identical');
  console.log(`${results.length} files compared, ${nonIdentical.length} non-identical.`);
  if (nonIdentical.length) {
    for (const r of nonIdentical) {
      console.log(`  ${r.status.padEnd(18)} ${r.file}${r.diffPct !== undefined ? `  (${r.diffPct}%, ${r.diffPixels}px)` : ''}`);
    }
  }
  console.log(`summary written to ${summaryPath}`);
  process.exit(nonIdentical.length ? 1 : 0);
}

main();
