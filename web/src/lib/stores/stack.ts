/**
 * Loop-stack store — two independent, client-only, in-memory panes, each an
 * ordered list of pending prompt cards ("loops"). Pure ops (add/remove/
 * duplicate/reorder/insert, plus the keyed pane dispatch) are exported
 * standalone for unit testing, then wrapped by a Svelte `writable` below,
 * mirroring the layout-core.ts / layout.ts split.
 *
 * UI-2 scope: nothing here talks to the backend. `cardToTaskPayload` is the
 * one honesty-preserving bridge — a pure mapping from a card's guardrails/
 * config onto the real `createTask()` shape (see `$lib/api`), proving the
 * WIRED fields round-trip correctly, even though nothing calls `createTask`
 * yet (run-stack execution is still a stub — see `RunMenu.svelte`).
 */
import { writable } from 'svelte/store';
import type { CreateTaskOptions } from '$lib/api';

// ── Types ─────────────────────────────────────────────────────────────────────

/** One rung of the eval ladder a card carries. */
export type EvalTier = 'base' | 'test' | 'judge' | 'suite';

/** A single named eval, either the full catalog or a card's on-set. */
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

/** Policy applied when a card's loop iteration fails. Mirrors `OnFail`
 *  (`crates/lopi-core/src/loop_config.rs`) — WIRED via `on_fail`. */
export type OnFail = 'stop' | 'continue' | 'backoff';

/** Per-run token-budget preset. Backend gap: no `CreateTaskRequest` field
 *  backs this yet — client-only intent, same as `branch`/`autonomy`. */
export type Budget = 'auto' | '200k' | 'none';

/** A card's run-limit guardrails. `gate`/`until`/`onFail` are WIRED to the
 *  real `CreateTaskOptions.gate` / `.until` / `.on_fail` fields
 *  (`crates/lopi-core/src/loop_config.rs`, landed PR #62). */
export interface Guardrails {
  gate: boolean;
  gateCmd: string;
  until: boolean;
  untilCmd: string;
  onFail: OnFail;
  /** Backend gap: no budget field exists on `CreateTaskRequest` yet. */
  budget: Budget;
}

/** Freshly-initialized guardrails — every card gets its own object (never a
 *  shared reference) so editing one card can't leak into another. */
export function defaultGuardrails(): Guardrails {
  return { gate: false, gateCmd: '', until: false, untilCmd: '', onFail: 'stop', budget: 'auto' };
}

/** The five preset schedule cadences a card can pick, plus a raw-cron escape
 *  hatch. Matches the settled mockup's frequency chip row. */
export type CronFreq = 'every minute' | 'hourly' | 'daily' | 'weekly' | 'custom';

/** Three-letter weekday tags, matching cron's day-of-week vocabulary. */
export type Dow = 'Sun' | 'Mon' | 'Tue' | 'Wed' | 'Thu' | 'Fri' | 'Sat';

/** A card's schedule. `raw` is the standard 5-field cron string — WIRED,
 *  mirrors `ScheduleEntry.cron` (`crates/lopi-core/src/config.rs`). The
 *  preset fields (`freq`/`hour12`/`min`/`ampm`/`dow`) are the two-way-synced
 *  UI state `raw` derives from; editing `raw` directly flips `freq` to
 *  `'custom'`. */
export interface CronConfig {
  freq: CronFreq;
  hour12: number;
  min: number;
  ampm: 'AM' | 'PM';
  dow: Dow;
  raw: string;
}

/** Freshly-initialized cron config — every card gets its own object. */
export function defaultCron(): CronConfig {
  return { freq: 'daily', hour12: 2, min: 0, ampm: 'AM', dow: 'Mon', raw: '0 2 * * *' };
}

/** Per-loop overrides of the pane defaults (model/effort/repo/branch/
 *  autonomy). `undefined` on any field means "inherit the pane default".
 *  `model`/`effort`/`repo` are WIRED (real `CreateTaskRequest` fields);
 *  `branch`/`autonomy` are client-only — backend gap, not yet exposed. */
export interface CardConfig {
  model?: string;
  effort?: string;
  repo?: string;
  branch?: string;
  autonomy?: string;
}

/** A card's lifecycle state. Client-only this slice — nothing transitions a
 *  card out of `'idle'` yet, since run-stack actions are stubbed (no
 *  pause/drain/bump signals exist server-side). */
export type CardStatus = 'idle' | 'queued' | 'running' | 'done';

/** The backend default iteration ceiling (`default_max_iterations()` in
 *  `crates/lopi-core/src/loop_config.rs`) — the value a fresh card starts
 *  from before anyone touches the iteration pill or guardrails stepper. */
export const DEFAULT_MAX_ITERATIONS = 25;

/** Floor a stepper will not go below without wrapping to infinite. */
export const MAX_ITERATIONS_FLOOR = 2;

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
  /** The eval suite this card carries — baseline always present. */
  evals: EvalRef[];
  status: CardStatus;
  /** Hard iteration ceiling. `0` = infinite (mirrors backend `max_iterations`
   *  sentinel). The cardbar iteration pill and the guardrails max-iter
   *  stepper both read/write this same field. */
  maxIterations: number;
  /** Live progress while `status === 'running'` — `undefined` otherwise. */
  iteration?: { current: number; total: number };
  scheduled: boolean;
  cron: CronConfig;
  guardrails: Guardrails;
  config: CardConfig;
  /** Set once the card is actually submitted as a task. Never set this
   *  slice — see `cardToTaskPayload`'s doc comment. */
  taskId?: string;
}

// ── Preset catalog (client-side static config this slice) ───────────────────

/** Baseline eval — always present, on every card, rendered dashed/dimmed. */
export const BASELINE_EVAL: EvalRef = { name: 'execution ok', tier: 'base' };

/** The full pickable eval catalog for the evals popover checklist. Baseline
 *  is first and locked-on; everything else toggles freely. */
export const EVAL_CATALOG: EvalRef[] = [
  BASELINE_EVAL,
  { name: 'tests pass', tier: 'test' },
  { name: 'unit', tier: 'test' },
  { name: 'integration', tier: 'test' },
  { name: 'benchmark gate', tier: 'test' },
  { name: '30-run gate', tier: 'test' },
  { name: 'code review', tier: 'judge' },
  { name: 'beats-best', tier: 'judge' },
  { name: 'vuln scan', tier: 'suite' },
  { name: 'adversarial', tier: 'suite' }
];

/** Suite shortcuts — clicking one turns on every named eval (baseline stays
 *  implicit). Matches the settled mockup's KCQF/security/research buttons. */
export const EVAL_SUITES: Record<string, string[]> = {
  kcqf: ['tests pass', 'code review', 'vuln scan', 'adversarial'],
  security: ['vuln scan', 'adversarial'],
  research: ['code review']
};

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
    evals: preset ? preset.evals : [BASELINE_EVAL],
    status: 'idle',
    maxIterations: parsed.loopN ?? DEFAULT_MAX_ITERATIONS,
    scheduled: false,
    cron: defaultCron(),
    guardrails: defaultGuardrails(),
    config: parsed.repo ? { repo: parsed.repo } : {}
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

/** Clone a card in place, immediately after the original. Resets run state
 *  (`status`/`iteration`/`taskId`) on the clone — a duplicate is a fresh,
 *  never-run loop. No-op if the id isn't present. */
export function duplicateCard(cards: StackCard[], id: string): StackCard[] {
  const idx = cards.findIndex((c) => c.id === id);
  if (idx === -1) return cards;
  const clone: StackCard = {
    ...cards[idx],
    id: makeId(),
    status: 'idle',
    iteration: undefined,
    taskId: undefined
  };
  const next = [...cards];
  next.splice(idx + 1, 0, clone);
  return next;
}

/** Move the card at `from` to index `to`. Out-of-range indices are a no-op.
 *  `to` is interpreted in the *post-removal* array — see
 *  `moveCardBeforeOrAfter` for the drag-and-drop-friendly variant that
 *  works in terms of "before/after this other card" instead. */
export function reorderCard(cards: StackCard[], from: number, to: number): StackCard[] {
  if (from < 0 || from >= cards.length || to < 0 || to >= cards.length) return cards;
  const next = [...cards];
  const [moved] = next.splice(from, 1);
  next.splice(to, 0, moved);
  return next;
}

/** Drag-and-drop-friendly reorder: move the card at `fromIndex` to just
 *  before/after the card currently at `targetIndex` (both indices from the
 *  *original* array, as read off the drag/drop DOM elements). A no-op when
 *  dropping a card onto itself. */
export function moveCardBeforeOrAfter(
  cards: StackCard[],
  fromIndex: number,
  targetIndex: number,
  before: boolean
): StackCard[] {
  if (fromIndex === targetIndex) return cards;
  const to = fromIndex < targetIndex ? (before ? targetIndex - 1 : targetIndex) : before ? targetIndex : targetIndex + 1;
  return reorderCard(cards, fromIndex, to);
}

/** Insert a card at a specific index, clamped into range. */
export function insertCardAt(cards: StackCard[], index: number, card: StackCard): StackCard[] {
  const next = [...cards];
  const clamped = Math.max(0, Math.min(index, next.length));
  next.splice(clamped, 0, card);
  return next;
}

/** Patch a single card by id with a shallow merge. No-op if the id isn't
 *  present. Callers pass fully-formed nested objects (e.g. a whole new
 *  `guardrails`) rather than deep-merging here, so popovers stay in control
 *  of exactly what changed. */
export function patchCard(cards: StackCard[], id: string, patch: Partial<StackCard>): StackCard[] {
  const idx = cards.findIndex((c) => c.id === id);
  if (idx === -1) return cards;
  const next = [...cards];
  next[idx] = { ...next[idx], ...patch };
  return next;
}

// ── Eval-set ops (pure, tested) ────────────────────────────────────────────────

/** Toggle one named eval in a card's on-set. The baseline never toggles off. */
export function toggleEval(evals: EvalRef[], name: string): EvalRef[] {
  if (name === BASELINE_EVAL.name) return evals;
  if (evals.some((e) => e.name === name)) return evals.filter((e) => e.name !== name);
  const found = EVAL_CATALOG.find((e) => e.name === name);
  return found ? [...evals, found] : evals;
}

/** Turn on every eval named in a suite shortcut; already-on evals are left
 *  alone (never duplicated). */
export function applySuite(evals: EvalRef[], suiteNames: string[]): EvalRef[] {
  const missing = suiteNames
    .filter((n) => !evals.some((e) => e.name === n))
    .map((n) => EVAL_CATALOG.find((e) => e.name === n))
    .filter((e): e is EvalRef => !!e);
  return missing.length ? [...evals, ...missing] : evals;
}

// ── Iteration stepper (pure, tested) ──────────────────────────────────────────

/** Step a card's `maxIterations` by `delta` (±1 from the pill/guardrails
 *  stepper). Floors at `MAX_ITERATIONS_FLOOR`; stepping below it wraps to
 *  the infinite sentinel (`0`). Stepping up from infinite skips straight to
 *  the floor rather than landing on `1`. */
export function stepMaxIterations(current: number, delta: number): number {
  if (current === 0) return delta > 0 ? MAX_ITERATIONS_FLOOR : 0;
  const next = current + delta;
  return next < MAX_ITERATIONS_FLOOR ? 0 : next;
}

/** Display text for a card's iteration ceiling (`∞` for the sentinel). */
export function maxIterationsLabel(maxIterations: number): string {
  return maxIterations === 0 ? '∞' : String(maxIterations);
}

// ── Active-state predicates (pure, drive cardbar highlighting) ────────────────

export function guardActive(g: Guardrails): boolean {
  return g.gate || g.until;
}

export function evalActive(card: StackCard): boolean {
  return card.evals.length > 1;
}

export function configActive(card: StackCard, defaults: { model: string; effort: string; repo: string; branch: string; autonomy: string }): boolean {
  const c = card.config;
  return (
    (c.model ?? defaults.model) !== defaults.model ||
    (c.effort ?? defaults.effort) !== defaults.effort ||
    (c.repo ?? defaults.repo) !== defaults.repo ||
    (c.branch ?? defaults.branch) !== defaults.branch ||
    (c.autonomy ?? defaults.autonomy) !== defaults.autonomy
  );
}

// ── Cron helpers (pure, tested) ────────────────────────────────────────────────

const DOW_TO_NUM: Record<Dow, number> = { Sun: 0, Mon: 1, Tue: 2, Wed: 3, Thu: 4, Fri: 5, Sat: 6 };

function to24Hour(hour12: number, ampm: 'AM' | 'PM'): number {
  const h = hour12 % 12;
  return ampm === 'PM' ? h + 12 : h;
}

/** Derive the standard 5-field cron string from a preset cadence. Returns
 *  `c.raw` verbatim when `freq === 'custom'` — the raw field is the source
 *  of truth once the operator has typed one directly. */
export function buildCronString(c: CronConfig): string {
  switch (c.freq) {
    case 'every minute':
      return '* * * * *';
    case 'hourly':
      return `${c.min} * * * *`;
    case 'daily':
      return `${c.min} ${to24Hour(c.hour12, c.ampm)} * * *`;
    case 'weekly':
      return `${c.min} ${to24Hour(c.hour12, c.ampm)} * * ${DOW_TO_NUM[c.dow]}`;
    case 'custom':
      return c.raw;
  }
}

function matchesCronField(field: string, value: number): boolean {
  if (field === '*') return true;
  return field.split(',').some((part) => {
    const step = part.match(/^\*\/(\d+)$/);
    if (step) return value % parseInt(step[1], 10) === 0;
    return parseInt(part, 10) === value;
  });
}

/** Search forward minute-by-minute from `from` for the next `count` times a
 *  standard 5-field cron expression fires. Bounded to ~40 days of search so
 *  an unsatisfiable expression (e.g. Feb 30) can't spin forever — returns
 *  fewer than `count` results in that case rather than blocking. Supports
 *  wildcards, exact numbers, comma lists, and step values (every Nth unit)
 *  per field; unknown syntax (or a non-5-field string) yields no results
 *  rather than throwing. */
export function computeNextRuns(cronExpr: string, from: Date, count = 3): Date[] {
  const fields = cronExpr.trim().split(/\s+/);
  if (fields.length !== 5) return [];
  const [minF, hourF, domF, monF, dowF] = fields;
  const results: Date[] = [];
  const cursor = new Date(from.getTime());
  cursor.setSeconds(0, 0);
  cursor.setMinutes(cursor.getMinutes() + 1);
  const limitMinutes = 60 * 24 * 40;
  for (let i = 0; i < limitMinutes && results.length < count; i++) {
    if (
      matchesCronField(minF, cursor.getMinutes()) &&
      matchesCronField(hourF, cursor.getHours()) &&
      matchesCronField(domF, cursor.getDate()) &&
      matchesCronField(monF, cursor.getMonth() + 1) &&
      matchesCronField(dowF, cursor.getDay())
    ) {
      results.push(new Date(cursor.getTime()));
    }
    cursor.setMinutes(cursor.getMinutes() + 1);
  }
  return results;
}

/** Human-readable echo of a cron config's cadence. */
export function cronHuman(c: CronConfig): string {
  const mm = String(c.min).padStart(2, '0');
  switch (c.freq) {
    case 'every minute':
      return 'every minute';
    case 'hourly':
      return `every hour at :${mm}`;
    case 'daily':
      return `every day at ${c.hour12}:${mm} ${c.ampm}`;
    case 'weekly':
      return `every ${c.dow} at ${c.hour12}:${mm} ${c.ampm}`;
    case 'custom':
      return 'custom cron';
  }
}

// ── Read-only summary lines (hide-inactive text, matches the settled mockup) ──

/** The schedule line shown when `card.scheduled`. */
export function scheduleSummary(card: StackCard): string {
  return cronHuman(card.cron);
}

/** The guardrails line shown when `gate || until`. */
export function guardSummary(card: StackCard): string {
  const g = card.guardrails;
  const parts: string[] = [];
  if (g.gate) parts.push('gate');
  if (g.until) parts.push('until');
  parts.push(`budget:${g.budget}`);
  parts.push(`max ${maxIterationsLabel(card.maxIterations)}`);
  return parts.join(' · ');
}

/** The evals line shown when more than the baseline is on: a count plus
 *  "baseline + N more", matching the settled mockup's at-rest phrasing. */
export function evalsSummary(card: StackCard): string {
  const n = card.evals.length;
  if (n <= 1) return '1 check · baseline only';
  return `${n} checks · baseline + ${n - 1} more`;
}

// ── Backend round-trip (WIRED fields → real CreateTaskOptions shape) ──────────

/** Pane-level defaults a card's `config` overrides fall back to. */
export interface PaneDefaults {
  model: string;
  effort: string;
  repo: string;
}

/** The `createTask(goal, repo, priority, options)` payload a card would
 *  submit as, resolving `config` overrides against pane defaults. Pure and
 *  total — this is the "round-trips through `api.ts`" contract for the
 *  WIRED guardrail/config fields (`§3` of the UI-2 brief), proven by unit
 *  test even though no run-stack action calls `createTask` yet (that needs
 *  the pause/drain/bump signals called out in `NEXT.md`). */
export function cardToTaskPayload(
  card: StackCard,
  defaults: PaneDefaults
): { goal: string; repo: string; priority: string; options: CreateTaskOptions } {
  const options: CreateTaskOptions = {
    model: card.config.model ?? defaults.model,
    effort: card.config.effort ?? defaults.effort,
    max_iterations: card.maxIterations,
    on_fail: card.guardrails.onFail,
    // Backend-1 — lets the response's `duplicate_of ?? id` (see
    // `api.ts::effectiveTaskId`) be traced straight back to this card
    // regardless of any server-side dedup.
    client_ref: card.id
  };
  if (card.guardrails.gate) options.gate = card.guardrails.gateCmd;
  if (card.guardrails.until) options.until = card.guardrails.untilCmd;
  return {
    goal: card.goal,
    repo: card.config.repo ?? defaults.repo,
    priority: 'normal',
    options
  };
}

/** The `cardToTaskPayload` a card would submit under the "Run once" run-menu
 *  intent: identical resolution, but `max_iterations` is forced to `1`
 *  regardless of the card's own setting (including the `0` = ∞ sentinel) —
 *  a plan-level override applied only to the outgoing payload, never
 *  mutating the card's own stored `maxIterations`. */
export function cardToTaskPayloadForRunOnce(
  card: StackCard,
  defaults: PaneDefaults
): { goal: string; repo: string; priority: string; options: CreateTaskOptions } {
  const payload = cardToTaskPayload(card, defaults);
  return { ...payload, options: { ...payload.options, max_iterations: 1 } };
}

// ── Run-stack execution order + dry run (pure, tested) ────────────────────────

/** The order a pane's cards actually run in: bottom-of-stack (oldest,
 *  closest to executing) first, top (newest) last. The composer prepends
 *  new cards to index 0 (`addCard`), so a pane's array order is newest
 *  first — the reverse of execution order — matching the settled mockup's
 *  "new prompts prepend to the top; the stack flows down to the
 *  currently-executing loop at the bottom" pane chrome. */
export function executionOrder(cards: StackCard[]): StackCard[] {
  return [...cards].reverse();
}

/** One problem `dryRunStack` found with a specific card's configuration. */
export interface DryRunIssue {
  cardId: string;
  message: string;
}

/** One card's resolved plan entry, exactly as `dryRunStack` would submit
 *  it — never actually submitted. */
export interface DryRunPlanEntry {
  cardId: string;
  goal: string;
  repo: string;
  maxIterations: number;
}

/** The plan-validation result `dryRunStack` returns. */
export interface DryRunResult {
  valid: boolean;
  issues: DryRunIssue[];
  plan: DryRunPlanEntry[];
}

/** Validate a pane's execution plan without running anything: resolves
 *  every card's config against the pane defaults (the same resolution
 *  `cardToTaskPayload` does) in execution order, and flags configs that
 *  would fail at launch — an empty goal, or a guardrail toggled on with an
 *  empty command. Pure and total; never calls `createTask`. This is the
 *  run-menu's "Dry run" intent in full — there is no backend call to make,
 *  since validating a plan needs nothing the client doesn't already have. */
export function dryRunStack(cards: StackCard[], defaults: PaneDefaults): DryRunResult {
  const issues: DryRunIssue[] = [];
  const plan: DryRunPlanEntry[] = executionOrder(cards).map((card) => {
    const payload = cardToTaskPayload(card, defaults);
    if (!payload.goal.trim()) {
      issues.push({ cardId: card.id, message: 'goal is empty' });
    }
    if (card.guardrails.gate && !card.guardrails.gateCmd.trim()) {
      issues.push({ cardId: card.id, message: 'gate is enabled with an empty command' });
    }
    if (card.guardrails.until && !card.guardrails.untilCmd.trim()) {
      issues.push({ cardId: card.id, message: 'until is enabled with an empty command' });
    }
    return {
      cardId: card.id,
      goal: payload.goal,
      repo: payload.repo,
      maxIterations: payload.options.max_iterations ?? DEFAULT_MAX_ITERATIONS
    };
  });
  return { valid: issues.length === 0, issues, plan };
}

/** Attempt to bump (swap with its immediate neighbor) a not-yet-started
 *  card within an active stack run's remaining execution order. `cursor`
 *  is the index of the card currently running or about to run — it and
 *  everything at or before it are off-limits, matching the brief's "reject
 *  illegal transitions... with a clear error, not a silent no-op." Pure —
 *  the caller (`stores/stackRun.ts`) is responsible for reflecting the
 *  result back onto the pane's own card array. */
export function bumpInOrder(
  order: string[],
  cursor: number,
  cardId: string,
  direction: 'up' | 'down'
): { ok: true; order: string[] } | { ok: false; error: string } {
  const idx = order.indexOf(cardId);
  if (idx === -1) return { ok: false, error: 'card is not part of this run’s plan' };
  if (idx <= cursor) {
    return { ok: false, error: 'card is already running or finished — only queued cards can be bumped' };
  }
  const targetIdx = direction === 'up' ? idx - 1 : idx + 1;
  if (targetIdx <= cursor) {
    return { ok: false, error: 'cannot bump above the currently running card' };
  }
  if (targetIdx >= order.length) {
    return { ok: false, error: 'cannot bump past the end of the queue' };
  }
  const next = [...order];
  [next[idx], next[targetIdx]] = [next[targetIdx], next[idx]];
  return { ok: true, order: next };
}

// ── Pane store (keyed dispatch over the pure array ops) ───────────────────────

/** One independent stack pane — `key` is its stable identity for keyed ops. */
export interface StackPaneState {
  key: string;
  title: string;
  cards: StackCard[];
}

function makeDefaultPanes(): StackPaneState[] {
  return [
    { key: 's1', title: 'stack one', cards: [] },
    { key: 's2', title: 'stack two', cards: [] }
  ];
}

/** Apply a pure card-list transform to one pane by key, leaving every other
 *  pane's array reference untouched. No-op (same reference) for an unknown
 *  key. This is the keyed-dispatch primitive every pane op below composes
 *  with — the pre-flight's `stack.insert(stackKey, index, loop)` shape. */
export function applyToPaneCards(
  state: StackPaneState[],
  key: string,
  fn: (cards: StackCard[]) => StackCard[]
): StackPaneState[] {
  const idx = state.findIndex((p) => p.key === key);
  if (idx === -1) return state;
  const next = [...state];
  next[idx] = { ...next[idx], cards: fn(next[idx].cards) };
  return next;
}

/** Insert a card into a specific pane at `index`. This is `stack.insert`
 *  from the pre-flight gate — the one op UI-1 didn't need and UI-2's
 *  `StackConnector` "add between" block depends on. */
export function insertIntoPane(
  state: StackPaneState[],
  key: string,
  index: number,
  card: StackCard
): StackPaneState[] {
  return applyToPaneCards(state, key, (cards) => insertCardAt(cards, index, card));
}

/** The two active stack panes — client-only, in-memory, no persistence this
 *  slice. */
export const panes = writable<StackPaneState[]>(makeDefaultPanes());

export function addToPane(key: string, card: StackCard): void {
  panes.update((state) => applyToPaneCards(state, key, (cards) => addCard(cards, card)));
}
export function removeFromPane(key: string, id: string): void {
  panes.update((state) => applyToPaneCards(state, key, (cards) => removeCard(cards, id)));
}
export function duplicateInPane(key: string, id: string): void {
  panes.update((state) => applyToPaneCards(state, key, (cards) => duplicateCard(cards, id)));
}
export function reorderInPane(key: string, from: number, to: number): void {
  panes.update((state) => applyToPaneCards(state, key, (cards) => reorderCard(cards, from, to)));
}
export function reorderInPaneRelative(key: string, fromIndex: number, targetIndex: number, before: boolean): void {
  panes.update((state) =>
    applyToPaneCards(state, key, (cards) => moveCardBeforeOrAfter(cards, fromIndex, targetIndex, before))
  );
}
export function insertCardIntoPane(key: string, index: number, card: StackCard): void {
  panes.update((state) => insertIntoPane(state, key, index, card));
}
export function updateCardInPane(key: string, id: string, patch: Partial<StackCard>): void {
  panes.update((state) => applyToPaneCards(state, key, (cards) => patchCard(cards, id, patch)));
}
