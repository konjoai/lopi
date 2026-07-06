/**
 * Pure markdown-splitter tests — `npx tsx src/lib/render/markdown.test.ts`.
 * Only `splitMarkdown` is exercised (it is DOM-free); `renderProse` needs a
 * browser DOM and is covered by the live render gates instead.
 */
import { splitMarkdown } from './markdown';

let pass = 0;
let fail = 0;
function eq(actual: unknown, expected: unknown, name: string) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a === e) pass++;
  else {
    fail++;
    console.error(`✗ ${name}: expected ${e}, got ${a}`);
  }
}

// ── prose only ────────────────────────────────────────────────────────────────
eq(splitMarkdown('hello **world**'), [{ kind: 'prose', md: 'hello **world**' }], 'prose only');

// ── one fenced block with a language, prose either side ───────────────────────
{
  const segs = splitMarkdown('before\n```ts\nconst x = 1;\n```\nafter');
  eq(segs.length, 3, 'prose / code / prose');
  eq(segs[0], { kind: 'prose', md: 'before' }, 'leading prose');
  eq(segs[1], { kind: 'code', lang: 'ts', code: 'const x = 1;' }, 'code segment with lang');
  eq(segs[2], { kind: 'prose', md: 'after' }, 'trailing prose');
}

// ── fence with no language → empty lang string ────────────────────────────────
{
  const segs = splitMarkdown('```\nplain\n```');
  eq(segs, [{ kind: 'code', lang: '', code: 'plain' }], 'no-lang fence');
}

// ── unterminated fence runs to end of input as code ───────────────────────────
{
  const segs = splitMarkdown('```bash\nls -la\nrm x');
  eq(segs, [{ kind: 'code', lang: 'bash', code: 'ls -la\nrm x' }], 'unterminated fence is code');
}

// ── blank-only prose between blocks is dropped ────────────────────────────────
{
  const segs = splitMarkdown('```\na\n```\n\n\n```\nb\n```');
  eq(segs.length, 2, 'whitespace-only prose between fences is dropped');
  eq(segs[0], { kind: 'code', lang: '', code: 'a' }, 'first code');
  eq(segs[1], { kind: 'code', lang: '', code: 'b' }, 'second code');
}

// ── a diff fence keeps its lang for green/red rendering ───────────────────────
eq(
  splitMarkdown('```diff\n+added\n-removed\n```'),
  [{ kind: 'code', lang: 'diff', code: '+added\n-removed' }],
  'diff fence preserved'
);

console.log(`\nmarkdown: ${pass} passed, ${fail} failed`);
if (fail > 0) process.exit(1);
