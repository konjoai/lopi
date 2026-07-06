/**
 * Syntax highlighting via Shiki — a single lazily-created highlighter shared by
 * every code block. Shiki is loaded on first use (code-split, so the orb-only
 * pages never pay for it) and the dark theme matches the Claude.ai code style.
 *
 * Highlighting is async and debounced by callers (on block close, not per
 * token), so a long streaming transcript never thrashes the highlighter.
 */
import type { Highlighter } from 'shiki';

/** The themes/langs loaded once. `text` is the safe fallback for anything else. */
const THEME = 'github-dark';
const LANGS = [
  'bash',
  'shell',
  'json',
  'typescript',
  'javascript',
  'tsx',
  'jsx',
  'rust',
  'python',
  'go',
  'html',
  'css',
  'svelte',
  'yaml',
  'toml',
  'sql',
  'markdown',
  'diff'
];

let highlighterPromise: Promise<Highlighter> | null = null;

async function getHighlighter(): Promise<Highlighter> {
  if (!highlighterPromise) {
    highlighterPromise = import('shiki').then((shiki) =>
      shiki.createHighlighter({ themes: [THEME], langs: LANGS })
    );
  }
  return highlighterPromise;
}

/** Normalize a fence language to one Shiki has loaded, falling back to `text`. */
export function normalizeLang(lang: string): string {
  const l = lang.toLowerCase();
  if (l === 'sh' || l === 'zsh' || l === 'console') return 'bash';
  if (l === 'ts') return 'typescript';
  if (l === 'js') return 'javascript';
  if (l === 'py') return 'python';
  if (l === 'yml') return 'yaml';
  return LANGS.includes(l) ? l : 'text';
}

/**
 * Highlight `code` to a `<pre>` HTML string. Returns `null` on any failure (the
 * caller then keeps its plain-text fallback rather than crashing the pane).
 */
export async function highlight(code: string, lang: string): Promise<string | null> {
  try {
    const hl = await getHighlighter();
    return hl.codeToHtml(code, { lang: normalizeLang(lang), theme: THEME });
  } catch (err) {
    console.warn('[lopi] shiki highlight failed:', err);
    return null;
  }
}
