/**
 * Loop-stack store — an ordered, client-only, in-memory list of pending
 * prompt cards ("loops"). Pure array ops (add/remove/duplicate/reorder/
 * insert) are exported standalone for unit testing, then wrapped by a
 * Svelte `writable` below, mirroring the layout-core.ts / layout.ts split.
 *
 * UI-1 scope: nothing here persists or talks to the backend. A stack is a
 * queue of cards the operator is composing; running it is a later slice.
 */
import { writable } from 'svelte/store';

// ── Types ─────────────────────────────────────────────────────────────────────

/** One rung of the eval ladder a card carries. */
export type EvalTier = 'base' | 'test' | 'judge' | 'suite';

/** A single named eval attached to a card (static this slice — no run state). */
export interface EvalRef {
  name: string;
  tier: EvalTier;
}

/** The five built-in presets a card can be created from. */
export type PresetKey = 'research' | 'implement' | 'optimize' | 'ratchet' | 'benchmark';

/** A preset's fixed shape: its alias, keyword-suggestion triggers, and the
 *  eval suite it carries (baseline always first). */
export interface PresetDef {
  key: PresetKey;
  label: string;
  alias: string;
  keywords: string[];
  evals: EvalRef[];
}

/** One card in the stack — a loop-to-be. */
export interface StackCard {
  id: string;
  /** Set when the card was created from a preset (grid, chip, or `:alias`). */
  preset?: PresetKey;
  /** The goal text: a literal prompt, or the text following an alias/preset. */
  goal: string;
  /** The alias token (without the leading `:`), if any. */
  alias?: string;
  /** True when `goal` is a plain literal prompt, not an alias/preset spec. */
  literal: boolean;
  /** `@repo` override, without the `@`. */
  repo?: string;
  /** `xN` loop count; undefined means "no loop" (single run). */
  loopN?: number;
  /** The eval suite this card carries — baseline always present. */
  evals: EvalRef[];
}

// ── Preset catalog (client-side static config this slice) ───────────────────

/** Baseline eval — always present, on every card, rendered dashed/dimmed. */
export const BASELINE_EVAL: EvalRef = { name: 'execution ok', tier: 'base' };

export const PRESET_CATALOG: Record<PresetKey, PresetDef> = {
  research: {
    key: 'research',
    label: 'research',
    alias: ':research',
    keywords: ['research', 'investigate', 'explore', 'learn', 'study', 'survey'],
    evals: [BASELINE_EVAL, { name: 'code review', tier: 'judge' }]
  },
  implement: {
    key: 'implement',
    label: 'implement',
    alias: ':implement',
    keywords: ['add', 'build', 'implement', 'feature', 'create', 'gate', 'wire'],
    evals: [
      BASELINE_EVAL,
      { name: 'unit', tier: 'test' },
      { name: 'integration', tier: 'test' },
      { name: 'code review', tier: 'judge' },
      { name: 'vuln scan', tier: 'suite' },
      { name: 'adversarial', tier: 'suite' }
    ]
  },
  optimize: {
    key: 'optimize',
    label: 'optimize',
    alias: ':optimize',
    keywords: ['optimize', 'improve', 'speed', 'performance', 'faster', 'latency'],
    evals: [
      BASELINE_EVAL,
      { name: 'beats-best', tier: 'judge' },
      { name: '30-run gate', tier: 'test' },
      { name: 'adversarial', tier: 'suite' }
    ]
  },
  ratchet: {
    key: 'ratchet',
    label: 'ratchet',
    alias: ':ratchet',
    keywords: ['ratchet', 'self-improve', 'self improve', 'beats-best'],
    evals: [
      BASELINE_EVAL,
      { name: 'beats-best', tier: 'judge' },
      { name: 'adversarial', tier: 'suite' }
    ]
  },
  benchmark: {
    key: 'benchmark',
    label: 'benchmark',
    alias: ':benchmark',
    keywords: ['benchmark', 'measure', 'variance', 'throughput'],
    evals: [
      BASELINE_EVAL,
      { name: 'benchmark gate', tier: 'test' },
      { name: '30-run gate', tier: 'test' }
    ]
  }
};

export const PRESET_KEYS = Object.keys(PRESET_CATALOG) as PresetKey[];

function isPresetKey(s: string): s is PresetKey {
  return (PRESET_KEYS as string[]).includes(s);
}

/** Keyword-match a typed goal against the preset catalog. Highlight-only —
 *  callers must never auto-attach the result, only suggest it. Returns the
 *  first matching preset, or null when nothing matches. */
export function suggestPreset(text: string): PresetKey | null {
  const lower = text.toLowerCase();
  for (const key of PRESET_KEYS) {
    if (PRESET_CATALOG[key].keywords.some((kw) => lower.includes(kw))) return key;
  }
  return null;
}

// ── Composer grammar parser ───────────────────────────────────────────────────

/** The pieces a composer/CLI/Telegram string parses into. */
export interface ParsedInput {
  alias: string | null;
  goal: string;
  repo: string | null;
  loopN: number | null;
}

/** Parse `:alias "goal" @repo xN` (any subset, any order after the leading
 *  alias) into its parts. Pure and total — never throws. */
export function parseComposerInput(raw: string): ParsedInput {
  let text = raw.trim();
  let alias: string | null = null;
  let repo: string | null = null;
  let loopN: number | null = null;

  const aliasMatch = text.match(/^:(\S+)/);
  if (aliasMatch) {
    alias = aliasMatch[1];
    text = text.slice(aliasMatch[0].length).trim();
  }

  const repoMatch = text.match(/@(\S+)/);
  if (repoMatch && repoMatch.index !== undefined) {
    repo = repoMatch[1];
    text = (text.slice(0, repoMatch.index) + text.slice(repoMatch.index + repoMatch[0].length)).trim();
  }

  const loopMatch = text.match(/\bx(\d+)\b/i);
  if (loopMatch && loopMatch.index !== undefined) {
    loopN = parseInt(loopMatch[1], 10);
    text = (text.slice(0, loopMatch.index) + text.slice(loopMatch.index + loopMatch[0].length)).trim();
  }

  const quoted = text.match(/^"(.*)"$/);
  const goal = (quoted ? quoted[1] : text).trim();

  return { alias, goal, repo, loopN };
}

// ── Card factory ──────────────────────────────────────────────────────────────

function makeId(): string {
  return crypto.randomUUID();
}

/** Build a `StackCard` from raw composer text, optionally forcing a preset
 *  (grid card / chip click). When the text's own `:alias` names a known
 *  preset, that preset's eval suite attaches automatically — the same
 *  string works from any of the three creation-flow doors. */
export function buildCard(raw: string, explicitPreset?: PresetKey): StackCard {
  const parsed = parseComposerInput(raw);
  const aliasPreset = parsed.alias && isPresetKey(parsed.alias) ? parsed.alias : undefined;
  const presetKey = explicitPreset ?? aliasPreset;
  const preset = presetKey ? PRESET_CATALOG[presetKey] : undefined;

  return {
    id: makeId(),
    preset: presetKey,
    goal: parsed.goal,
    alias: parsed.alias ?? preset?.key,
    literal: !parsed.alias && !presetKey,
    repo: parsed.repo ?? undefined,
    loopN: parsed.loopN ?? undefined,
    evals: preset ? preset.evals : [BASELINE_EVAL]
  };
}

// ── Pure array ops (unit-tested directly) ─────────────────────────────────────

/** Prepend a card to the top of the stack. */
export function addCard(cards: StackCard[], card: StackCard): StackCard[] {
  return [card, ...cards];
}

/** Drop a card by id. No-op if the id isn't present. */
export function removeCard(cards: StackCard[], id: string): StackCard[] {
  return cards.filter((c) => c.id !== id);
}

/** Clone a card in place, immediately after the original. No-op if the id
 *  isn't present. */
export function duplicateCard(cards: StackCard[], id: string): StackCard[] {
  const idx = cards.findIndex((c) => c.id === id);
  if (idx === -1) return cards;
  const clone: StackCard = { ...cards[idx], id: makeId() };
  const next = [...cards];
  next.splice(idx + 1, 0, clone);
  return next;
}

/** Move the card at `from` to index `to`. Out-of-range indices are a no-op. */
export function reorderCard(cards: StackCard[], from: number, to: number): StackCard[] {
  if (from < 0 || from >= cards.length || to < 0 || to >= cards.length) return cards;
  const next = [...cards];
  const [moved] = next.splice(from, 1);
  next.splice(to, 0, moved);
  return next;
}

/** Insert a card at a specific index, clamped into range. */
export function insertCardAt(cards: StackCard[], index: number, card: StackCard): StackCard[] {
  const next = [...cards];
  const clamped = Math.max(0, Math.min(index, next.length));
  next.splice(clamped, 0, card);
  return next;
}

// ── Read-only summary lines (static text this slice) ─────────────────────────

/** The guardrails line shown at rest. Guardrail fields (budget, on-fail,
 *  schedule) aren't backed by any state yet — UI-2 owns editing them — so
 *  this renders the same static defaults every card would start from. */
export function guardrailsSummary(card: StackCard): string {
  const max = card.loopN != null ? String(card.loopN) : '∞';
  return `budget:auto · max ${max}`;
}

/** The evals line shown at rest: a count plus "baseline + N more", matching
 *  the settled mockup's at-rest phrasing. */
export function evalsSummary(card: StackCard): string {
  const n = card.evals.length;
  if (n <= 1) return '1 check · baseline only';
  return `${n} checks · baseline + ${n - 1} more`;
}

// ── Store wrapper ─────────────────────────────────────────────────────────────

/** The active stack — client-only, in-memory, no persistence this slice. */
export const stack = writable<StackCard[]>([]);

export function addToStack(card: StackCard): void {
  stack.update((cards) => addCard(cards, card));
}
export function removeFromStack(id: string): void {
  stack.update((cards) => removeCard(cards, id));
}
export function duplicateInStack(id: string): void {
  stack.update((cards) => duplicateCard(cards, id));
}
export function reorderInStack(from: number, to: number): void {
  stack.update((cards) => reorderCard(cards, from, to));
}
export function insertIntoStack(index: number, card: StackCard): void {
  stack.update((cards) => insertCardAt(cards, index, card));
}
