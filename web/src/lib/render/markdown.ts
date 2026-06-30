/**
 * Markdown utilities for the transcript renderer.
 *
 * The transcript needs component-level control over fenced code blocks (a
 * language label, a copy button, async syntax highlighting, diff coloring), so
 * rather than dumping one `{@html marked(...)}` blob we split the source at
 * fences: prose runs render as sanitized HTML, code runs render as `<CodeBlock>`
 * components. `splitMarkdown` is pure and unit-tested; `renderProse` touches the
 * DOM (DOMPurify) and is therefore browser-only.
 */
import { marked } from 'marked';
import DOMPurify from 'dompurify';

/** One run of the source: either prose (markdown) or a fenced code block. */
export type Segment =
  | { kind: 'prose'; md: string }
  | { kind: 'code'; lang: string; code: string };

const FENCE = /^(\s*)(`{3,}|~{3,})(.*)$/;

/**
 * Split markdown into alternating prose / fenced-code segments. Tolerant: an
 * unterminated fence runs to the end of input as code; non-code text is grouped
 * into prose segments verbatim for `renderProse`.
 */
export function splitMarkdown(source: string): Segment[] {
  const segments: Segment[] = [];
  const lines = source.split('\n');
  let prose: string[] = [];
  let i = 0;

  const flushProse = () => {
    if (prose.length === 0) return;
    const md = prose.join('\n');
    if (md.trim()) segments.push({ kind: 'prose', md });
    prose = [];
  };

  while (i < lines.length) {
    const open = FENCE.exec(lines[i]);
    if (!open) {
      prose.push(lines[i]);
      i++;
      continue;
    }
    flushProse();
    const lang = open[3].trim().split(/\s+/)[0] ?? '';
    const body: string[] = [];
    i++;
    while (i < lines.length && !FENCE.test(lines[i])) {
      body.push(lines[i]);
      i++;
    }
    i++; // consume closing fence (or run off the end harmlessly)
    segments.push({ kind: 'code', lang, code: body.join('\n') });
  }
  flushProse();
  return segments;
}

let configured = false;
function configureMarked() {
  if (configured) return;
  marked.setOptions({ gfm: true, breaks: true });
  configured = true;
}

/**
 * Render a prose markdown run to sanitized HTML. Browser-only (DOMPurify needs a
 * DOM). Fenced code is handled by the caller via `splitMarkdown`, so inline code
 * is all that survives here.
 */
export function renderProse(md: string): string {
  configureMarked();
  const raw = marked.parse(md, { async: false }) as string;
  return DOMPurify.sanitize(raw, { USE_PROFILES: { html: true } });
}
