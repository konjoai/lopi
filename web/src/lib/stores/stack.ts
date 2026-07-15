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
import type { Acceptance, AcceptanceCheck, CreateTaskOptions } from '$lib/api';
import { type StackDefaults, DEFAULT_STACK_DEFAULTS, defaultStackDefaults } from '$lib/stores/stackDefaults';
import { AUTO_MODEL, MODEL_OPTIONS, labelFor, type Option } from '$lib/stores/options';
import { resolveRepoToken } from '$lib/stores/repoMenu';
import { matches } from '$lib/stores/optionMenu';

// ── Types ─────────────────────────────────────────────────────────────────────

/** One rung of the eval ladder a card carries. */
export type EvalTier = 'base' | 'test' | 'judge' | 'suite';

/** A single named eval, either the full catalog or a card's on-set. */
export interface EvalRef {
  name: string;
  tier: EvalTier;
}

/** The built-in presets a card can be created from. */
export type PresetKey =
  | 'research'
  | 'implement'
  | 'optimize'
  | 'gain'
  | 'benchmark'
  | 'test'
  | 'killtest'
  | 'report';

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

/** Per-run token-budget preset. A3 — wired to the real `CreateTaskOptions.
 *  budget_tokens` field via `budgetToTokens`, which the runner meters against
 *  (stops with `StopReason::Budget` on exceed). */
export type Budget = 'auto' | '200k' | 'none';

/** Resolve a budget preset to the enforced per-loop token cap, or `undefined`
 *  when the preset sets no hard cap. `'200k'` → 200 000 tokens; `'auto'`
 *  inherits the repo/global budget and `'none'` is explicitly uncapped — both
 *  omit the field so the payload never claims a limit the loop won't enforce
 *  (the honesty rule the hidden budget badge was pulled for; see
 *  `StackConnector.svelte`). */
export function budgetToTokens(budget: Budget): number | undefined {
  return budget === '200k' ? 200_000 : undefined;
}

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

/** An Anthropic account rate-limit window MAXX's headroom gate can check —
 *  mirrors `lopi_core::LimitWindow`'s wire tags exactly. */
export type LimitWindow = 'five_hour' | 'seven_day';

/** A card's MAXX (opportunistic backlog dispatch) settings. `quietHours` and
 *  `windows`/`headroomGate` are the fixed policy this popover offers (no
 *  per-field editing in this sprint — see `MaxxPopover.svelte`'s doc comment)
 *  sent to `/api/maxx` when `enabled` flips on. Mirrors `CronConfig`'s
 *  per-card, always-present-object convention. */
export interface MaxxConfig {
  enabled: boolean;
  /** `(start, end)` local hours, e.g. `[23, 7]` for 11PM-7AM. */
  quietHours: [number, number];
  headroomGate: boolean;
  windows: LimitWindow[];
}

/** Freshly-initialized MAXX config — every card gets its own object. Matches
 *  the sample values in the locked popover design (11PM-7AM, both windows). */
export function defaultMaxx(): MaxxConfig {
  return { enabled: false, quietHours: [23, 7], headroomGate: true, windows: ['five_hour', 'seven_day'] };
}

/** Freshly-initialized cron config — every card gets its own object. */
export function defaultCron(): CronConfig {
  return { freq: 'daily', hour12: 2, min: 0, ampm: 'AM', dow: 'Mon', raw: '0 2 * * *' };
}

/** Per-loop overrides of the pane defaults (model/effort/repo/branch/
 *  autonomy). `undefined` on any field means "inherit the pane default".
 *  `model`/`effort`/`repo` are WIRED (real `CreateTaskRequest` fields);
 *  `autonomy` is client-only — backend gap, not yet exposed. `branch` has no
 *  field of its own but still reaches the server: both `paneSubmitPayload`
 *  (bare-pane launch) and `cardToTaskPayload` (run-stack execution) turn it
 *  into the same "Target branch: …" planning constraint. */
export interface CardConfig {
  model?: string;
  effort?: string;
  repo?: string;
  branch?: string;
  autonomy?: string;
}

/** A card's lifecycle state. `'draft'` is the pre-commit state of the pane's
 *  in-composer draft card (Creation-Flow-1) — it is never in `pane.cards`, is
 *  excluded from every run/loop-count/payload path (see `executionOrder`), and
 *  must be handled explicitly by any `CardStatus` consumer rather than falling
 *  through to a run path. The rest are the client-only run lifecycle. */
export type CardStatus = 'draft' | 'idle' | 'queued' | 'running' | 'done';

/** The default iteration ceiling a fresh card starts from. `0` = "off": the
 *  loop is disabled and the card runs a single pass (the card pill floors at
 *  0 and never reaches the backend's infinite sentinel — see
 *  `stepCardIterations`/`cardToTaskPayload`, which maps an off card to a
 *  single `max_iterations: 1` on the wire). A user dials this *up* from off to
 *  ask for repeats. */
export const DEFAULT_MAX_ITERATIONS = 0;

/** Floor the stack loop-count stepper will not go below without wrapping to
 *  infinite (`stepMaxIterations`). The *card* iteration pill uses its own
 *  off-at-zero stepper (`stepCardIterations`) and ignores this. */
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
  /** MAXX — opportunistic backlog dispatch. Independent of `scheduled`/
   *  `cron`: a card can have both a cron schedule and MAXX on at once. */
  maxx: MaxxConfig;
  /** The `/api/maxx` row id backing this card's MAXX toggle, once created.
   *  `undefined` until `enabled` is flipped on for the first time — never set
   *  by anything other than `MaxxPopover`'s CRUD wiring, and cleared on
   *  duplicate so a clone never shares its original's backend entry. */
  maxxEntryId?: string;
  guardrails: Guardrails;
  config: CardConfig;
  /** Set once the card is actually submitted as a task. Never set this
   *  slice — see `cardToTaskPayload`'s doc comment. */
  taskId?: string;
  /** Name of the template this card came from (provenance, not a binding).
   *  Records origin only — it survives edits to `goal`/`preset` and never
   *  tracks drift. `undefined` when the card came from no template. */
  tpl?: string;
  /** Which kind of template produced it — drives the provenance chip's color
   *  (`prompt` → sun chip replacing the alias chip; `stack` → violet chip
   *  alongside the alias chip). Set iff `tpl` is set. */
  tplKind?: 'prompt' | 'stack';
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
  // A3 — the gain gate and this preset share the word (renamed from
  // `:ratchet`; the legacy alias still resolves here, see `LEGACY_ALIASES`).
  gain: {
    key: 'gain',
    label: 'gain',
    alias: ':gain',
    keywords: ['gain', 'ratchet', 'self-improve', 'self improve', 'beats-best'],
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
  },
  test: {
    key: 'test',
    label: 'test',
    alias: ':test',
    keywords: ['test', 'verify', 'validate', 'confirm', 'prove', 'check'],
    evals: [
      BASELINE_EVAL,
      { name: 'tests pass', tier: 'test' },
      { name: 'integration', tier: 'test' },
      { name: 'code review', tier: 'judge' }
    ]
  },
  // Adversarial "try to break it" testing — distinct from `:test`'s
  // verification intent. Reuses the existing `adversarial`/`vuln scan` evals
  // rather than inventing a new eval type for kill-testing.
  killtest: {
    key: 'killtest',
    label: 'killtest',
    alias: ':killtest',
    keywords: ['killtest', 'kill test', 'break', 'destroy', 'adversarial', 'stress', 'fuzz', 'attack'],
    evals: [
      BASELINE_EVAL,
      { name: 'adversarial', tier: 'suite' },
      { name: 'vuln scan', tier: 'suite' },
      { name: '30-run gate', tier: 'test' }
    ]
  },
  // A documentation-deliverable preset (write an .md summarizing the latest
  // findings/session) — not a code-correctness suite, so its eval set mirrors
  // `:research`'s (baseline + judge-reviewed review, since there's no code
  // change to test/scan, just a write-up worth a review pass for accuracy).
  report: {
    key: 'report',
    label: 'report',
    alias: ':report',
    keywords: ['report', 'summarize', 'summary', 'findings', 'writeup', 'write up', 'docs'],
    evals: [BASELINE_EVAL, { name: 'code review', tier: 'judge' }]
  }
};

export const PRESET_KEYS = Object.keys(PRESET_CATALOG) as PresetKey[];

/** One-line human descriptions for the templates dropdown's presets section
 *  (Creation-Flow-1 §5). Kept beside the catalog so the web + macOS surfaces
 *  read the same copy. */
export const PRESET_DESCRIPTIONS: Record<PresetKey, string> = {
  research: 'explore & investigate — judge-reviewed',
  implement: 'build a feature — full test + review suite',
  optimize: 'improve speed — beats-best + 30-run gate',
  gain: 'self-improve — ratchet on beats-best',
  benchmark: 'measure variance — benchmark + 30-run gate',
  test: 'verify it works — full test suite + review',
  killtest: 'try to break it — adversarial + vuln scan + 30-run gate',
  report: 'write up findings — .md summary, judge-reviewed'
};

/** Legacy `:alias` tokens that map onto a renamed preset key, so old composer
 *  strings / saved cards keep working. A3 renamed `:ratchet` → `:gain`. */
const LEGACY_ALIASES: Record<string, PresetKey> = { ratchet: 'gain' };

/** Resolve a raw alias token (without the leading `:`) to a preset key, applying
 *  legacy renames. Returns `null` when it names no known preset. */
export function resolvePresetAlias(alias: string): PresetKey | null {
  if (isPresetKey(alias)) return alias;
  return LEGACY_ALIASES[alias] ?? null;
}

export interface AliasSuggestion {
  /** The full token, leading colon included — ready to write straight into
   *  the goal field (e.g. `:research`). */
  alias: string;
  label: string;
  hint: string;
}

/** Filtered alias suggestions for the goal input's autocomplete, given its
 *  *entire current value*. Only suggests while the field is still a bare
 *  `:token` with no space yet (`^:(\S*)$`) — once a space follows, the user
 *  has moved on to typing the goal text and this returns `[]`. Only
 *  canonical `PRESET_KEYS` are ever suggested; legacy aliases (e.g. the
 *  renamed `:ratchet`→`:gain`) still resolve on commit but never appear here,
 *  so the autocomplete never steers anyone toward a deprecated token. */
export function aliasAutocomplete(goalText: string): AliasSuggestion[] {
  const match = /^:(\S*)$/.exec(goalText);
  if (!match) return [];
  const query = match[1].toLowerCase();
  return PRESET_KEYS.filter((key) => key.toLowerCase().startsWith(query)).map((key) => ({
    alias: `:${key}`,
    label: PRESET_CATALOG[key].label,
    hint: PRESET_DESCRIPTIONS[key]
  }));
}

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

// ── Inline `/command` autocomplete ────────────────────────────────────────────
// Every prompt/stack setting gets a `:`/`@`/`/` alias, not just presets and
// repo: `/model`, `/effort`, `/branch`, `/autonomy`, `/eval` are value-pickers
// (mirrors the user's own suggested `/model/<autocomplete>` syntax — the
// level-2 token embeds the real value directly, so unlike `@repo` there's no
// label/path resolution step); `/guard`, `/schedule`, `/maxx`, `/goal` carry
// multi-field state that doesn't reduce to one inline value, so picking one
// just opens the existing popover for it (the composer component owns that
// action — this module only supplies the pure matching).

/** One inline `/command` definition. */
export interface InlineCommandDef {
  command: string;
  hint: string;
  /** `true` → typing `/command` then continues into a second `/command/value`
   *  token (see `commandValueAutocomplete`). `false` → selecting the command
   *  fires an immediate action (open a popover) with no value step. */
  isValuePicker: boolean;
}

/** Card-scope commands, typed into a loop's own goal field. */
export const CARD_COMMANDS: InlineCommandDef[] = [
  { command: 'model', hint: "override this loop's model", isValuePicker: true },
  { command: 'effort', hint: "override this loop's effort", isValuePicker: true },
  { command: 'branch', hint: "override this loop's target branch", isValuePicker: true },
  { command: 'autonomy', hint: "override this loop's autonomy level", isValuePicker: true },
  { command: 'eval', hint: 'toggle an eval suite (kcqf/security/research)', isValuePicker: true },
  { command: 'guard', hint: "open this loop's guardrails", isValuePicker: false },
  { command: 'schedule', hint: "open this loop's schedule", isValuePicker: false },
  { command: 'maxx', hint: 'open MAXX backlog dispatch', isValuePicker: false }
];

/** Stack-scope commands, typed into the stack's own command bar
 *  (`StackControlDock.svelte`) — same vocabulary, writes to `pane.config`
 *  instead of a card's `config`. No `maxx` (per-card only); adds `loop`
 *  (chain loop count) and `goal` (run-until-goal), which have no card-level
 *  analog. */
export const STACK_COMMANDS: InlineCommandDef[] = [
  { command: 'model', hint: 'stack default model', isValuePicker: true },
  { command: 'effort', hint: 'stack default effort', isValuePicker: true },
  { command: 'branch', hint: 'stack default branch', isValuePicker: true },
  { command: 'autonomy', hint: 'stack default autonomy', isValuePicker: true },
  { command: 'loop', hint: 'stack loop count', isValuePicker: true },
  { command: 'eval', hint: 'toggle a stack eval suite', isValuePicker: true },
  { command: 'guard', hint: 'open stack guardrails', isValuePicker: false },
  { command: 'schedule', hint: 'open the stack schedule', isValuePicker: false },
  { command: 'goal', hint: 'open run-until-goal', isValuePicker: false }
];

/** A level-1 `/command` suggestion — the bare command name, not yet a value. */
export interface CommandSuggestion {
  token: string;
  command: string;
  label: string;
  hint: string;
}

/** Level 1: filtered command-name suggestions for a trailing `/token` — the
 *  same trailing-word grammar `repoAutocomplete` uses, generalized over a
 *  caller-supplied command list (card vs. stack scope differ). */
export function commandAutocomplete(goalText: string, commands: InlineCommandDef[]): CommandSuggestion[] {
  const match = /(?:^|\s)\/([a-z]*)$/.exec(goalText);
  if (!match) return [];
  const q = match[1].toLowerCase();
  return commands
    .filter((c) => c.command.startsWith(q))
    .map((c) => ({ token: `/${c.command}`, command: c.command, label: c.command, hint: c.hint }));
}

/** A level-2 `/command/value` suggestion. */
export interface CommandValueSuggestion {
  token: string;
  label: string;
  hint: string;
  value: string;
}

/** Level 2: once a value-picker command has been chosen (the composer tracks
 *  this as its own `pendingCommand` state), matches a trailing
 *  `/command/value` token against whatever catalog applies to `command`. */
export function commandValueAutocomplete(goalText: string, command: string, options: Option[]): CommandValueSuggestion[] {
  const match = new RegExp(`(?:^|\\s)/${command}/(\\S*)$`).exec(goalText);
  if (!match) return [];
  const q = match[1].toLowerCase();
  return options
    .filter((o) => matches(o, q))
    .map((o) => ({ token: `/${command}/${o.value}`, label: o.label, hint: o.hint ?? '', value: o.value }));
}

/** `/eval`'s value catalog is the suite-shortcut names (`kcqf`/`security`/
 *  `research`), not individual eval names — those contain spaces (`"vuln
 *  scan"`, `"code review"`), which the trailing-token grammar can't carry.
 *  Bulk-toggling a suite is the useful, space-free case; per-eval toggling
 *  stays a popover click. */
export function evalSuiteOptions(): Option[] {
  return Object.keys(EVAL_SUITES).map((name) => ({ value: name, label: name }));
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
 *  string works from any of the three creation-flow doors.
 *
 *  `repoOptions` resolves a parsed `@token`'s label (e.g. `"konjoai/lopi"`)
 *  to the real absolute path via `resolveRepoToken` before it lands on
 *  `config.repo` — `CreateTaskRequest.repo` reaches `git2::Repository::open`
 *  with no server-side resolution, so a label stored here would fail to
 *  launch. Defaults to `[]` (no resolution, label stored as-is) for callers
 *  with no live catalog to resolve against (`makeDraft`, tests, templates);
 *  live composer commits always pass the fetched catalog — see
 *  `finalizeDraft`/`commitDraft`. */
export function buildCard(raw: string, explicitPreset?: PresetKey, repoOptions: Option[] = []): StackCard {
  const parsed = parseComposerInput(raw);
  const aliasPreset = parsed.alias ? resolvePresetAlias(parsed.alias) ?? undefined : undefined;
  const presetKey = explicitPreset ?? aliasPreset;
  const preset = presetKey ? PRESET_CATALOG[presetKey] : undefined;
  const resolvedRepo = parsed.repo ? resolveRepoToken(parsed.repo, repoOptions) : null;

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
    maxx: defaultMaxx(),
    guardrails: defaultGuardrails(),
    config: resolvedRepo ? { repo: resolvedRepo } : {}
  };
}

/** A fresh draft card — the pre-commit composer replacement pinned to the top
 *  of every pane (Creation-Flow-1). Same shape as any card but `status:
 *  'draft'`, so it renders through the one `StackCard.svelte` component with a
 *  draft branch rather than a forked `DraftCard`. Never enters `pane.cards`. */
export function makeDraft(): StackCard {
  return { ...buildCard(''), status: 'draft' };
}

/** True once a draft carries enough to commit: an alias, non-empty goal, or a
 *  template origin. Drives the draft's `hot` (teal-border) state and enables
 *  the `+ add` button. Pure so it reads identically on web and macOS. */
export function draftIsHot(draft: StackCard): boolean {
  return !!(draft.alias || draft.goal.trim() || draft.tpl);
}

// ── Templates (presets + prompt/stack templates, pure + tested) ───────────────

/** A saved single-loop template: a preset and/or alias plus goal text. Client
 *  provenance only (`tpl`/`tplKind` on the produced card) — applying it fills a
 *  draft, it does not bind the card to the template afterward. */
export interface PromptTemplate {
  id: string;
  name: string;
  preset?: PresetKey;
  alias?: string;
  goal: string;
}

/** A saved multi-loop chain template. `loops` is serialized **bottom-first**
 *  (execution order — first-to-run first) by `stackTemplateFromCards`, so
 *  `applyStackTemplate` round-trips it back into the same run order. */
export interface StackTemplate {
  id: string;
  name: string;
  loops: Array<{ preset?: PresetKey; alias?: string; goal: string }>;
}

/** Attach a preset to a card in place: sets `preset`/`alias`/`evals` from the
 *  catalog and clears any template provenance (picking a bare preset is not a
 *  template origin). Leaves `goal` and every configured facet untouched. */
export function applyPreset(card: StackCard, key: PresetKey): StackCard {
  const p = PRESET_CATALOG[key];
  return { ...card, preset: key, alias: p.key, evals: p.evals, literal: false, tpl: undefined, tplKind: undefined };
}

/** Fill a card from a prompt template: preset/alias/goal/evals from the
 *  catalog, plus prompt provenance (`tpl`/`tplKind: 'prompt'`). The preset (if
 *  any) still drives evals/config exactly as a hand-picked preset would. */
export function applyPromptTemplate(card: StackCard, tpl: PromptTemplate): StackCard {
  const presetKey = tpl.preset ?? (tpl.alias ? resolvePresetAlias(tpl.alias) ?? undefined : undefined);
  const preset = presetKey ? PRESET_CATALOG[presetKey] : undefined;
  return {
    ...card,
    preset: presetKey,
    alias: tpl.alias ?? preset?.key,
    goal: tpl.goal,
    evals: preset ? preset.evals : [BASELINE_EVAL],
    literal: false,
    tpl: tpl.name,
    tplKind: 'prompt'
  };
}

/** Build one committed card from a stack-template loop, stamped with stack
 *  provenance. Mirrors `buildCard`'s preset resolution, but from a structured
 *  loop rather than composer text (loops carry no `@repo`/`×N`). */
function cardFromLoop(loop: { preset?: PresetKey; alias?: string; goal: string }, tplName: string): StackCard {
  const presetKey = loop.preset ?? (loop.alias ? resolvePresetAlias(loop.alias) ?? undefined : undefined);
  const preset = presetKey ? PRESET_CATALOG[presetKey] : undefined;
  return {
    id: makeId(),
    preset: presetKey,
    goal: loop.goal,
    alias: loop.alias ?? preset?.key,
    literal: !presetKey && !loop.alias,
    evals: preset ? preset.evals : [BASELINE_EVAL],
    status: 'idle',
    maxIterations: DEFAULT_MAX_ITERATIONS,
    scheduled: false,
    cron: defaultCron(),
    maxx: defaultMaxx(),
    guardrails: defaultGuardrails(),
    config: {},
    tpl: tplName,
    tplKind: 'stack'
  };
}

/** Drop a whole chain template into a pane's cards. `addCard` prepends
 *  (newest on top; the **bottom** card is oldest and runs first), so to land
 *  the template's **first loop at the bottom** the loops are prepended in
 *  reverse. Round-trips with `stackTemplateFromCards` — see its doc + the
 *  bottom-first unit test. */
export function applyStackTemplate(cards: StackCard[], tpl: StackTemplate): StackCard[] {
  const loopCards = tpl.loops.map((l) => cardFromLoop(l, tpl.name));
  loopCards.reverse();
  return [...loopCards, ...cards];
}

/** Serialize a card into a reusable prompt template (provenance is not carried
 *  — a template is a fresh origin, not a copy of another template's lineage). */
export function promptTemplateFromCard(card: StackCard, name: string): PromptTemplate {
  return { id: makeId(), name, preset: card.preset, alias: card.alias, goal: card.goal };
}

/** Serialize a pane's cards into a stack template **bottom-first** (execution
 *  order) so `applyStackTemplate` restores the identical run order — the
 *  easiest thing to get backwards, hence the explicit round-trip test. */
export function stackTemplateFromCards(cards: StackCard[], name: string): StackTemplate {
  return {
    id: makeId(),
    name,
    loops: executionOrder(cards).map((c) => ({ preset: c.preset, alias: c.alias, goal: c.goal }))
  };
}

/** Commit a draft into a real card. A draft configured via the dropdown
 *  (preset or template applied) commits as-is; a still-raw draft honors the
 *  inline `:alias @repo ×N` tokens typed into its goal field — the power-user
 *  path the retired composer supported. Only ever flips `status` to `'idle'`;
 *  never mutates the pane. `repoOptions` resolves any inline `@token` label
 *  to its real path — see `buildCard`'s doc comment; pass the live catalog
 *  whenever one is available (`commitDraft` always does). */
export function finalizeDraft(draft: StackCard, repoOptions: Option[] = []): StackCard {
  if (draft.preset || draft.tpl) return { ...draft, status: 'idle' };
  const parsed = parseComposerInput(draft.goal);
  if (!parsed.alias && !parsed.repo && parsed.loopN === null) {
    return { ...draft, status: 'idle', goal: parsed.goal, literal: true };
  }
  const built = buildCard(draft.goal, undefined, repoOptions);
  return {
    ...built,
    id: draft.id,
    status: 'idle',
    scheduled: draft.scheduled,
    cron: draft.cron,
    maxx: draft.maxx,
    maxxEntryId: draft.maxxEntryId,
    guardrails: draft.guardrails,
    config: { ...built.config, ...draft.config }
  };
}

/** Whether committing a card should seed the pane's own stack-level repo
 *  default — only the first time, while the default is still the cold-start
 *  `''` ("auto") sentinel. A later card with a different `@repo` never
 *  clobbers an explicit choice (own or user-picked). Pulled out of
 *  `commitDraft` so the rule is unit-testable without touching the `panes`
 *  store. */
export function adoptRepoDefaultIfUnset(defaults: StackDefaults, committed: StackCard): StackDefaults {
  if (defaults.repo || !committed.config.repo) return defaults;
  return { ...defaults, repo: committed.config.repo };
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
    taskId: undefined,
    // A clone never shares its original's backend /api/maxx row — reset to
    // off so the popover doesn't show "enabled" with nothing behind it.
    maxx: { ...cards[idx].maxx, enabled: false },
    maxxEntryId: undefined
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

/** Step the *stack* loop-count by `delta` (±1 from the pill/guardrails
 *  stepper). Three states: `1` = off (run the chain once, no repeat), a
 *  literal count `2..N` (no ceiling — keeps incrementing), and the infinite
 *  sentinel `0` (run until the goal/guardrails stop it). Cycles
 *  `1 (off) → 2 → ... → N → 0 (∞) → 1`; there is no way to land on a value
 *  below `1` other than the infinite sentinel itself. */
export function stepMaxIterations(current: number, delta: number): number {
  if (current === 0) return delta > 0 ? 1 : 0;
  if (current === 1) return delta > 0 ? MAX_ITERATIONS_FLOOR : 0;
  const next = current + delta;
  return next < MAX_ITERATIONS_FLOOR ? 1 : next;
}

/** Display text for the *stack* loop-count pill: `∞` for the infinite
 *  sentinel, `off` for a single run with no chain repeat, the plain number
 *  otherwise. The stack pill keeps the wrap-to-infinite behavior so a
 *  goal-pursuing chain can still be set to run "until met". */
export function maxIterationsLabel(maxIterations: number): string {
  if (maxIterations === 0) return '∞';
  if (maxIterations === 1) return 'off';
  return String(maxIterations);
}

/** Step a *card's* `maxIterations` by `delta`. Unlike the stack pill, the
 *  card floors at `0` = "off" (single run) and never wraps to the infinite
 *  sentinel — stepping down past 0 stays off. */
export function stepCardIterations(current: number, delta: number): number {
  return Math.max(0, current + delta);
}

/** Display text for a *card's* iteration pill — `off` when the loop is
 *  disabled (`0`), the plain number otherwise. */
export function cardIterationsLabel(maxIterations: number): string {
  return maxIterations === 0 ? 'off' : String(maxIterations);
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

/** The bolded descriptor half of the MAXX summary line — e.g. "quiet hours +
 *  headroom", matching the locked design's "on · **quiet hours + headroom**"
 *  sample text (the "on ·" prefix is rendered unbolded by the caller). */
export function maxxSummary(card: StackCard): string {
  // `quietHours` is a fixed policy field, always present once MAXX exists on
  // a card — there's no UI to unset it independently of `enabled` in this
  // sprint (see `MaxxPopover.svelte`'s doc comment), so it's always listed.
  const parts: string[] = ['quiet hours'];
  if (card.maxx.headroomGate) parts.push('headroom');
  return parts.join(' + ');
}

/** The guardrails line shown when `gate || until`. */
export function guardSummary(card: StackCard): string {
  const g = card.guardrails;
  const parts: string[] = [];
  if (g.gate) parts.push('gate');
  if (g.until) parts.push('until');
  parts.push(`budget:${g.budget}`);
  parts.push(`max ${cardIterationsLabel(card.maxIterations)}`);
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

/** Pane-level defaults a card's `config` overrides fall back to. `branch` is
 *  optional here (real callers pass the fuller `StackDefaults`, which always
 *  has one) purely so a minimal `{model, effort, repo}` literal still
 *  satisfies this type. */
export interface PaneDefaults {
  model: string;
  effort: string;
  repo: string;
  branch?: string;
}

/** Compile a card's `evals` checklist into a real
 *  [`Acceptance`](../api.ts) the backend's tiered eval executor scores
 *  against (A1) — the bridge that finally makes the eval UI execute instead
 *  of being inert intent. The tier→spec mapping enforces the
 *  objective-to-deterministic routing rule:
 *
 *  - `base`/`test` (objective) → a single deterministic `execution_ok` check —
 *    tests + lint are machine-checkable, so they never reach the judge.
 *  - `judge` → one judge check whose rubric criteria are the selected judge
 *    evals' names (genuine judgment, one model call, not per-eval).
 *  - `suite` → one `suite` check per selected suite eval.
 *
 *  Returns `undefined` when there is nothing to check, so the loop falls back
 *  to the legacy `score.passed()` gate — behavior is unchanged for a card that
 *  somehow carries no evals. */
export function evalsToAcceptance(evals: EvalRef[]): Acceptance | undefined {
  const checks: AcceptanceCheck[] = [];
  const hasDeterministic = evals.some((e) => e.tier === 'base' || e.tier === 'test');
  if (hasDeterministic) {
    checks.push({ tier: 'base', spec: { kind: 'execution_ok' }, weight: 1, required: true });
  }
  const judgeNames = evals.filter((e) => e.tier === 'judge').map((e) => e.name);
  if (judgeNames.length > 0) {
    checks.push({
      tier: 'judge',
      spec: { kind: 'judge', rubric: { name: 'ui-evals', criteria: judgeNames } },
      weight: 1,
      required: true
    });
  }
  for (const suite of evals.filter((e) => e.tier === 'suite')) {
    checks.push({ tier: 'suite', spec: { kind: 'suite', name: suite.name }, weight: 1, required: true });
  }
  return checks.length > 0 ? { checks } : undefined;
}

/** The `createTask(goal, repo, priority, options)` payload a card would
 *  submit as, resolving `config` overrides against pane defaults. Pure and
 *  total — this is the "round-trips through `api.ts`" contract for the
 *  WIRED guardrail/config fields (`§3` of the UI-2 brief), proven by unit
 *  test independent of its real call site: `stores/stackRun.ts`'s sequencer
 *  calls it (via `cardToTaskPayload`/`cardToTaskPayloadForRunOnce`) once per
 *  card, in execution order, as part of Backend-1's run-stack execution. */
export function cardToTaskPayload(
  card: StackCard,
  defaults: PaneDefaults
): { goal: string; repo: string; priority: string; options: CreateTaskOptions } {
  const options: CreateTaskOptions = {
    effort: card.config.effort ?? defaults.effort,
    // `0` = "off" on the card pill → a single pass on the wire (never the
    // backend's `0` = infinite sentinel). Any positive N passes through.
    max_iterations: card.maxIterations === 0 ? 1 : card.maxIterations,
    on_fail: card.guardrails.onFail,
    // Backend-1 — lets the response's `duplicate_of ?? id` (see
    // `api.ts::effectiveTaskId`) be traced straight back to this card
    // regardless of any server-side dedup.
    client_ref: card.id
  };
  // `auto` (`AUTO_MODEL`) means "no override" — omit `model` so the backend's
  // `select_model` size heuristic runs, instead of sending the literal string
  // `"auto"` through to `task.model`'s override check (which the CLI would
  // reject as `--model auto`).
  const resolvedModel = card.config.model ?? defaults.model;
  if (resolvedModel && resolvedModel !== AUTO_MODEL) options.model = resolvedModel;
  if (card.guardrails.gate) options.gate = card.guardrails.gateCmd;
  if (card.guardrails.until) options.until = card.guardrails.untilCmd;
  // A3 — a budget preset that sets a real cap flows to the metered
  // `budget_tokens`; inherit/unlimited presets omit it (no inert claim).
  const budgetTokens = budgetToTokens(card.guardrails.budget);
  if (budgetTokens !== undefined) options.budget_tokens = budgetTokens;
  // A1 — compile the card's evals into a real acceptance goal so eval
  // execution finally happens; omitted when the card carries no checks.
  const acceptance = evalsToAcceptance(card.evals);
  if (acceptance) options.acceptance = acceptance;
  // `branch` has no `CreateTaskRequest` field of its own — same encoding
  // `paneSubmitPayload` uses, so a card's branch override reaches the
  // server on the run-stack path too, not just the bare-pane launch.
  const branch = (card.config.branch ?? defaults.branch)?.trim();
  if (branch) options.constraints = [`Target branch: ${branch}`];
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

/** A bare-prompt launch from a Forge-style pane composer. Unify-1 collapses
 *  Forge's old `postTask` call into the same `createTask` path a stack card's
 *  launch takes — this is the pure builder for the "one prompt, no stack
 *  chrome" case. */
export interface PaneLaunch {
  /** The composer's free-text prompt. */
  goal: string;
  /** Repo the pane targets (empty falls back to the server's configured repo). */
  repo: string;
  /** Scheduling priority; `'normal'` when unset. */
  priority?: string;
  /** Worker-model override; omitted from the payload when unset. */
  model?: string;
  /** Reasoning-effort hint; omitted from the payload when unset. */
  effort?: string;
  /** Target branch; surfaced as a planning constraint when set (the same
   *  channel the retired `postTask` used), omitted otherwise. */
  branch?: string;
}

/** The `createTask(goal, repo, priority, options)` payload a bare pane prompt
 *  submits as. Deliberately a *bare* payload — it carries only what the pane's
 *  launch controls actually set (goal/repo/priority + optional model/effort +
 *  optional branch constraint) and forces none of the stack-loop semantics
 *  (`max_iterations`/`on_fail`/`gate`/`until`/`acceptance`/`client_ref`) that
 *  `cardToTaskPayload` adds. So a bare prompt stays a bare prompt while still
 *  flowing through the one unified launch call. Pure and total; proven equal to
 *  `cardToTaskPayload`'s shape on the shared fields by `stack.test.ts`. */
export function paneSubmitPayload(
  launch: PaneLaunch
): { goal: string; repo: string; priority: string; options: CreateTaskOptions } {
  const options: CreateTaskOptions = {};
  // `auto` means "no override" — see `cardToTaskPayload`'s matching comment.
  if (launch.model && launch.model !== AUTO_MODEL) options.model = launch.model;
  if (launch.effort) options.effort = launch.effort;
  const branch = launch.branch?.trim();
  if (branch) options.constraints = [`Target branch: ${branch}`];
  return {
    goal: launch.goal,
    repo: launch.repo,
    priority: launch.priority || 'normal',
    options
  };
}

// ── Run-stack execution order + dry run (pure, tested) ────────────────────────

/** The order a pane's cards actually run in: bottom-of-stack (oldest,
 *  closest to executing) first, top (newest) last. The composer prepends
 *  new cards to index 0 (`addCard`), so a pane's array order is newest
 *  first — the reverse of execution order — matching the settled mockup's
 *  "new prompts prepend to the top; the stack flows down to the
 *  currently-executing loop at the bottom" pane chrome. */
export function executionOrder(cards: StackCard[]): StackCard[] {
  // Defensive: a draft card is never in `pane.cards`, but any code path that
  // resolves a run plan must still refuse to schedule one (Creation-Flow-1
  // §1.1 — never let `'draft'` fall through to a run path).
  return cards.filter((c) => c.status !== 'draft').reverse();
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

// ── Stack-level config (Stack-1: loop-count, schedule, guardrails, evals,
//    config-defaults — the purple stack control area's data) ─────────────────

/** The chain-level analogue of a loop's `Guardrails`. Deliberately narrower —
 *  no `gate`/`until`: those are shell preconditions/exit-conditions around a
 *  *single* task's own retry loop (`crates/lopi-core/src/loop_config.rs`),
 *  executed server-side inside one agent run. There is no server-side
 *  concept of "the whole client-side stack," so a chain-wide gate/until
 *  command has nowhere to actually run — inventing one here would be
 *  exactly the "inert control that looks enforced" the brief rules out.
 *  `onFail` is WIRED into the chain sequencer (`stores/stackRun.ts`'s
 *  `advance`) — a real, observable client behavior, just re-scoped from
 *  "how one task retries" to "what the chain does when a card fails."
 *  `budget` stays client-only/unenforced, same honesty rule as the per-loop
 *  budget (hidden from view — see `StackConnector.svelte`'s doc comment). */
export interface StackGuardrails {
  onFail: OnFail;
  budget: Budget;
}

/** Freshly-initialized chain guardrails — every stack gets its own object. */
export function defaultStackGuardrails(): StackGuardrails {
  return { onFail: 'stop', budget: 'auto' };
}

/** The stack control area's placement. `'dock'` is a collapsible strip
 *  pinned to the pane's base — a slim always-visible summary + run button
 *  that expands upward to the full controls (the shipped default, matching
 *  `docs/ui/lopi-stack-control-area.html`'s settled "V3" option).
 *  `'sticky'` is the always-fully-expanded, permanently-pinned variant from
 *  the same mockup ("option 1") — its CSS ships in `StackControlDock.svelte`
 *  today even though nothing sets this to `'sticky'`, exactly the
 *  `SIDEBAR_MODE`/`stores/nav.ts` precedent: flipping this one constant
 *  later is the entire migration, no rebuild. Not exposed as a user-facing
 *  toggle (out of scope this sprint). */
export const STACK_CONTROL_MODE: 'dock' | 'sticky' = 'dock';

/** A chain run's default iteration count — `1` (run once through), not the
 *  per-loop `DEFAULT_MAX_ITERATIONS` — a fresh stack shouldn't implicitly
 *  repeat itself. Reuses the same `0` = infinite sentinel and the same
 *  `stepMaxIterations`/`maxIterationsLabel` helpers as the per-loop
 *  iteration pill (the brief's "reuse the exact loop controls, just scoped
 *  to the stack"). When a stack is *pursuing a goal* (B1), this same
 *  `loopCount` is re-read as the `max_chain_loops` ceiling — how many times
 *  the whole chain may re-run before giving up (`0` = until the goal or a
 *  softer stop reason fires). */
export const DEFAULT_STACK_LOOP_COUNT = 1;

/** B1 — the default no-progress tolerance for a goal-pursuing stack: stop with
 *  `no_progress` after this many consecutive chain-runs that don't gain on the
 *  stack metric. Mirrors the spirit of the per-loop `no_progress_limit`
 *  (`crates/lopi-core`), one scope up. */
export const DEFAULT_NO_PROGRESS_LIMIT = 3;

/** B1 — the stack's run-until-goal facet. When `pursue` is on and the stack
 *  carries acceptance beyond the baseline (`stackEvalActive`), `runStack`
 *  re-runs the whole chain until the stack acceptance passes (`goal_met`) or a
 *  chain-scope stop reason fires (see `stores/stackGoal.ts`). Off by default,
 *  so a stack with no goal behaves exactly as before — additive and
 *  backward-compatible, the same honesty rule the rest of Stack-1 follows. */
export interface StackGoal {
  /** Run-until-goal on/off. */
  pursue: boolean;
  /** Consecutive non-gaining chain-runs tolerated before a `no_progress` stop;
   *  `0` disables the no-progress detector. */
  noProgressLimit: number;
}

/** Freshly-initialized goal facet — every stack gets its own object. */
export function defaultStackGoal(): StackGoal {
  return { pursue: false, noProgressLimit: DEFAULT_NO_PROGRESS_LIMIT };
}

/** Stack-level config — the purple control area's full state. `scheduled`/
 *  `cron` are STUBBED (rendered, editable, never actually fired — see
 *  `stores/stackRun.ts`'s doc comment on why whole-chain cron needs backend
 *  work this sprint doesn't have). `evals` is CLIENT-ONLY (chain-acceptance
 *  intent only; eval execution doesn't exist anywhere yet). `defaults` is
 *  WIRED — resolved into every loop's real `CreateTaskOptions` at the
 *  payload step (`cardToTaskPayload`'s existing `card.config.field ??
 *  defaults.field` already *is* the `loop ?? stack.default ?? DEF`
 *  precedence rule, since a stack's own `defaults` object is always a
 *  concrete `StackDefaults` seeded from `DEFAULT_STACK_DEFAULTS` — there is
 *  no "unset" stack-default state to fall further through). */
export interface StackConfig {
  loopCount: number;
  scheduled: boolean;
  cron: CronConfig;
  guardrails: StackGuardrails;
  evals: EvalRef[];
  defaults: StackDefaults;
  /** B1 — run-until-goal. WIRED into the chain sequencer
   *  (`stores/stackRun.ts`): with `pursue` on and acceptance beyond baseline,
   *  the chain re-runs until the stack acceptance passes or a stack stop
   *  reason fires. Additive — default `pursue: false` reproduces today's
   *  fixed-`loopCount` behavior exactly. */
  goal: StackGoal;
}

/** Freshly-initialized stack config — every pane gets its own objects
 *  throughout (never a shared reference), matching `buildCard`'s per-card
 *  convention. */
export function defaultStackConfig(): StackConfig {
  return {
    loopCount: DEFAULT_STACK_LOOP_COUNT,
    scheduled: false,
    cron: defaultCron(),
    guardrails: defaultStackGuardrails(),
    evals: [BASELINE_EVAL],
    defaults: defaultStackDefaults(),
    goal: defaultStackGoal()
  };
}

// ── Stack-level active-state predicates + summaries (hide-inactive, mirrors
//    the per-loop `guardActive`/`evalActive`/`configActive` family) ──────────

/** A chain guardrails facet reads "active" once it's been set away from the
 *  do-nothing default (`onFail: 'stop'` is indistinguishable from "never
 *  touched" — there's no separate enabled toggle at the chain level the way
 *  gate/until have one per-loop). */
export function stackGuardActive(g: StackGuardrails): boolean {
  return g.onFail !== 'stop';
}

export function stackEvalActive(config: StackConfig): boolean {
  return config.evals.length > 1;
}

/** B1 — the goal facet reads "active" once run-until-goal is switched on. The
 *  facet only *does* anything when the stack also carries acceptance beyond the
 *  baseline (`stackEvalActive`) — `pursue` with nothing to check is inert, so
 *  the sequencer requires both (see `runStack`). */
export function stackGoalActive(config: StackConfig): boolean {
  return config.goal.pursue;
}

/** True only when run-until-goal is on *and* there is a real acceptance to
 *  pursue — the exact condition `runStack` gates chain re-running on, surfaced
 *  as a pure predicate so the dock can render "pursuing goal" honestly (never
 *  when the toggle is on but there's nothing to check). */
export function stackPursuesGoal(config: StackConfig): boolean {
  return config.goal.pursue && stackEvalActive(config);
}

/** The goal summary line for the dock: the target (chain acceptance) plus the
 *  chain-loop ceiling it pursues within, mirroring the other `stack*Summary`
 *  helpers' terse "part · part" shape. */
export function stackGoalSummary(config: StackConfig): string {
  const ceiling = config.loopCount === 0 ? 'until met' : `≤${config.loopCount} chain-runs`;
  return `pursue chain acceptance · ${ceiling}`;
}

/** The stack's own defaults read "active" once any field has moved off the
 *  app-wide baseline — parallels `configActive`'s per-card comparison,
 *  just against `DEFAULT_STACK_DEFAULTS` instead of a passed-in pane
 *  default. */
export function stackDefaultsActive(defaults: StackDefaults): boolean {
  return (
    defaults.model !== DEFAULT_STACK_DEFAULTS.model ||
    defaults.effort !== DEFAULT_STACK_DEFAULTS.effort ||
    defaults.repo !== DEFAULT_STACK_DEFAULTS.repo ||
    defaults.branch !== DEFAULT_STACK_DEFAULTS.branch ||
    defaults.autonomy !== DEFAULT_STACK_DEFAULTS.autonomy
  );
}

/** The chain guardrails summary line: on-fail policy + budget preset,
 *  mirroring `guardSummary`'s "`part · part`" shape. */
export function stackGuardSummary(g: StackGuardrails): string {
  return `${g.onFail} · budget:${g.budget}`;
}

/** The chain evals summary line, mirroring `evalsSummary`'s phrasing but
 *  naming this "chain acceptance" (the mockup's own wording) rather than
 *  "loop validation" — these are checks against the whole stack's outcome,
 *  not one card's. */
export function stackEvalsSummary(config: StackConfig): string {
  const n = config.evals.length;
  if (n <= 1) return '1 check · baseline only';
  return `${n} checks · chain acceptance`;
}

/** The stack defaults summary line: which model every loop inherits, per
 *  the mockup's "default model X · every loop inherits" copy. Uses the
 *  option's display label rather than the raw wire value — load-bearing for
 *  `auto`, whose raw value would otherwise render the bare sentinel string
 *  instead of a real display string. */
export function stackDefaultsSummary(defaults: StackDefaults): string {
  return `model ${labelFor(MODEL_OPTIONS, defaults.model)} · every loop inherits`;
}

/** §1's second precedence rule, load-bearing and pure: while the stack
 *  drives cadence (either it's on its own schedule, or it's set to loop the
 *  whole chain more than once), a card's own `scheduled` flag must not be
 *  presented as independently active — its cron never fires on its own
 *  inside a governed stack. This never mutates a card's stored `scheduled`/
 *  `cron` (so toggling stack governance off instantly restores the card's
 *  prior schedule display) — it's purely a *rendering* rule, exactly what
 *  the brief means by "don't render a per-loop schedule as active when the
 *  stack governs it." */
export function perLoopScheduleGoverned(config: StackConfig): boolean {
  return config.scheduled || config.loopCount !== 1;
}

// ── Pane store (keyed dispatch over the pure array ops) ───────────────────────

/** One independent stack pane — `key` is its stable identity for keyed ops.
 *  `draft` is the pane's live composer-replacement card (Creation-Flow-1),
 *  pinned above `cards` and never a member of it. */
export interface StackPaneState {
  key: string;
  title: string;
  cards: StackCard[];
  config: StackConfig;
  draft: StackCard;
}

function makeDefaultPanes(): StackPaneState[] {
  return [
    { key: 's1', title: 'stack one', cards: [], config: defaultStackConfig(), draft: makeDraft() },
    { key: 's2', title: 'stack two', cards: [], config: defaultStackConfig(), draft: makeDraft() }
  ];
}

/** True when a pane should render as a *bare* box — top composer and an idle
 *  orb, nothing else: no inter-card connector, no purple stack control dock.
 *  Only an empty pane is bare; a pane earns its full stack chrome (dock +
 *  connectors) as soon as it holds its first card, so the run/schedule/
 *  guardrails/goal controls are visible from the very first prompt. */
export function paneIsBare(pane: StackPaneState): boolean {
  return pane.cards.length < 1;
}

/** A fresh, empty stack pane with its own config object and a unique key. */
export function makeBlankStack(title = 'new stack'): StackPaneState {
  return { key: makeId(), title, cards: [], config: defaultStackConfig(), draft: makeDraft() };
}

/** Append a fresh blank pane — the create-from-scratch path `deleteStack`'s
 *  doc comment anticipated ("revisit once pane creation exists"). Pure twin of
 *  `duplicateStack`. */
export function addStack(state: StackPaneState[]): StackPaneState[] {
  return [...state, makeBlankStack()];
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

// ── Stack-level ops (Stack-1 §2 pre-flight: none of these existed before —
//    UI-2/Backend-1/Shell-1 only ever operated on a fixed two-pane array).
//    Pure, tested, and isolated per pane exactly like the card ops above. ──

/** Clone a whole stack — pane title, config, and every card — in place,
 *  immediately after the original. Mirrors `duplicateCard`'s reset: every
 *  cloned card gets a fresh id and its run state wiped (`status: 'idle'`,
 *  no `iteration`/`taskId`), and the clone gets a fresh pane key + its own
 *  `config` object (never a shared reference with the original, so editing
 *  one stack's defaults/guardrails/schedule can't leak into the other's).
 *  No-op if the key isn't present. */
export function duplicateStack(state: StackPaneState[], key: string): StackPaneState[] {
  const idx = state.findIndex((p) => p.key === key);
  if (idx === -1) return state;
  const original = state[idx];
  const clone: StackPaneState = {
    key: makeId(),
    title: `${original.title} copy`,
    cards: original.cards.map((c) => ({ ...c, id: makeId(), status: 'idle', iteration: undefined, taskId: undefined })),
    config: {
      ...original.config,
      cron: { ...original.config.cron },
      guardrails: { ...original.config.guardrails },
      evals: [...original.config.evals],
      defaults: { ...original.config.defaults },
      goal: { ...original.config.goal }
    },
    // A duplicated stack starts with its own empty draft — the original's
    // in-progress draft is not part of what "duplicate" means to copy.
    draft: makeDraft()
  };
  const next = [...state];
  next.splice(idx + 1, 0, clone);
  return next;
}

/** Copy another currently-open pane's cards into this one, replacing
 *  whatever cards this pane already has — the "saved stacks" section of the
 *  stack-scope templates menu (Stack-Templates-1 §5). This is deliberately
 *  **not** a real stack library: nothing is persisted beyond the two panes
 *  already in memory, so "saved" only ever means "currently open elsewhere."
 *  Real durability is `Persistence-1`, a separate sprint. Every copied card
 *  gets a fresh id and its run state wiped, mirroring `duplicateStack`'s
 *  per-card reset. No-op if either key is missing or they're the same pane. */
export function loadStackCardsInto(state: StackPaneState[], targetKey: string, sourceKey: string): StackPaneState[] {
  if (targetKey === sourceKey) return state;
  const source = state.find((p) => p.key === sourceKey);
  if (!source) return state;
  return applyToPaneCards(state, targetKey, () =>
    source.cards.map((c) => ({ ...c, id: makeId(), status: 'idle', iteration: undefined, taskId: undefined }))
  );
}

/** Move the stack at `from` to index `to`. Out-of-range indices are a
 *  no-op — the exact same shape as `reorderCard`, just one level up (panes
 *  instead of cards within a pane). */
export function reorderStacks(state: StackPaneState[], from: number, to: number): StackPaneState[] {
  if (from < 0 || from >= state.length || to < 0 || to >= state.length) return state;
  const next = [...state];
  const [moved] = next.splice(from, 1);
  next.splice(to, 0, moved);
  return next;
}

/** Drag-and-drop-friendly stack reorder: move the pane at `fromIndex` to
 *  just before/after the pane currently at `targetIndex` — the pane-level
 *  twin of `moveCardBeforeOrAfter`, used by the stack control dock's drag
 *  handle. */
export function moveStackBeforeOrAfter(
  state: StackPaneState[],
  fromIndex: number,
  targetIndex: number,
  before: boolean
): StackPaneState[] {
  if (fromIndex === targetIndex) return state;
  const to = fromIndex < targetIndex ? (before ? targetIndex - 1 : targetIndex) : before ? targetIndex : targetIndex + 1;
  return reorderStacks(state, fromIndex, to);
}

/** Drop a stack by key. Refuses to delete the last remaining pane — there
 *  is no "add a new stack" affordance anywhere in the app yet (panes are
 *  only ever created via `duplicateStack`), so emptying the array would
 *  strand the user with no way back short of a full page reload. A
 *  deliberate floor, not an oversight; revisit once pane creation exists. */
export function deleteStack(state: StackPaneState[], key: string): StackPaneState[] {
  if (state.length <= 1) return state;
  return state.filter((p) => p.key !== key);
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

/** Patch a pane's draft card with a shallow merge (Creation-Flow-1). Same
 *  contract as `updateCardInPane` — callers pass fully-formed nested objects.
 *  The draft is edited in place until committed via `commitDraft`. */
export function updateDraftInPane(key: string, patch: Partial<StackCard>): void {
  panes.update((state) => {
    const idx = state.findIndex((p) => p.key === key);
    if (idx === -1) return state;
    const next = [...state];
    next[idx] = { ...next[idx], draft: { ...next[idx].draft, ...patch } };
    return next;
  });
}

/** Commit a pane's draft into a real (`'idle'`) card at the top of the stack
 *  (`addCard` prepends), then mint a fresh empty draft. The one transition a
 *  draft ever makes out of `'draft'`. No-op for an unknown key. */
export function commitDraft(key: string, repoOptions: Option[] = []): void {
  panes.update((state) => {
    const idx = state.findIndex((p) => p.key === key);
    if (idx === -1) return state;
    const pane = state[idx];
    const next = [...state];
    const finalized = finalizeDraft(pane.draft, repoOptions);
    next[idx] = {
      ...pane,
      cards: addCard(pane.cards, finalized),
      config: { ...pane.config, defaults: adoptRepoDefaultIfUnset(pane.config.defaults, finalized) },
      draft: makeDraft()
    };
    return next;
  });
}

/** Replace a pane's draft with a fresh empty one — the templates dropdown's
 *  "clear" and the reset after a stack template drops its own cards. */
export function resetDraft(key: string): void {
  updateDraftInPane(key, makeDraft());
}

/** Drop a whole stack template into a pane at once, in the correct run order
 *  (`applyStackTemplate` — first loop at the bottom). */
export function applyStackTemplateToPane(key: string, tpl: StackTemplate): void {
  panes.update((state) => applyToPaneCards(state, key, (cards) => applyStackTemplate(cards, tpl)));
}

/** Patch a pane's stack-level config with a shallow merge — the config
 *  drawer/popovers' write path, mirroring `updateCardInPane`'s contract
 *  (callers pass fully-formed nested objects; this never deep-merges). */
export function updateStackConfig(key: string, patch: Partial<StackConfig>): void {
  panes.update((state) => {
    const idx = state.findIndex((p) => p.key === key);
    if (idx === -1) return state;
    const next = [...state];
    next[idx] = { ...next[idx], config: { ...next[idx].config, ...patch } };
    return next;
  });
}

export function duplicateStackInPanes(key: string): void {
  panes.update((state) => duplicateStack(state, key));
}
export function loadStackCardsIntoPane(targetKey: string, sourceKey: string): void {
  panes.update((state) => loadStackCardsInto(state, targetKey, sourceKey));
}
export function reorderStacksInPanes(fromIndex: number, targetIndex: number, before: boolean): void {
  panes.update((state) => moveStackBeforeOrAfter(state, fromIndex, targetIndex, before));
}
export function deleteStackFromPanes(key: string): void {
  panes.update((state) => deleteStack(state, key));
}
export function addStackPane(): void {
  panes.update((state) => addStack(state));
}
