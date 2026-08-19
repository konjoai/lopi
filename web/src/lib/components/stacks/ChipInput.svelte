<!--
  ChipInput — round 2, item 2 (corrected direction). A single-line
  `contenteditable` field that renders resolved `:alias`/`@repo`/
  `;command/value`/`×N` tokens as inline atomic chips, mixed with plain text,
  in place of the raw characters they replace — a mention/tag-pill input
  (Linear/Notion-style), not a plain `<textarea>`.

  Ownership split, deliberately narrow:
  - The PARENT (`StackCard.svelte`'s draft branch, `StackControlDock.svelte`'s
    `.cmdbarwrap`) still owns the plain-text value, the autocomplete matching
    (`aliasAutocomplete`/`repoAutocomplete`/`commandAutocomplete`/
    `commandValueAutocomplete`) and selection (`selectAlias`/`selectRepo`/
    `selectCommand` and their cmdbar equivalents) exactly as before — none of
    that logic changed for round 2, only *where* the result renders.
  - This component only knows how to (a) render a caller-supplied
    `GoalSegment[]` (from `stores/stack.ts`'s `tokenizeGoalChips`) as
    text-nodes-plus-atomic-chip-spans, and (b) serialize its own DOM content
    back to a plain string on every edit.

  Rendering is 100% imperative (`buildDom`), never through Svelte's own
  `{#each}` — the moment Svelte's reactive block-diffing and the browser's
  native contenteditable DOM mutations (typing, IME composition, chip
  boundary deletion) touch the same nodes, the cursor jumps mid-keystroke.
  So `segments` only ever drives a *rebuild* when `value` changed from
  something other than what this component itself just emitted (i.e. an
  external write — a selection, a commit reset, a template load) — normal
  typing lets the browser own its own DOM undisturbed, and this component
  only reads it back out via `input`.
-->
<script lang="ts">
  import { tick } from 'svelte';
  import type { GoalSegment } from '$lib/stores/stack';

  export let value: string;
  /** Pre-tokenized by the caller via `tokenizeGoalChips(value, COMMANDS)` —
   *  kept as a prop rather than computed in here so this component never
   *  needs to know which command catalog (card vs. stack scope) applies. */
  export let segments: GoalSegment[];
  export let onInput: (value: string) => void;
  export let onFocus: () => void = () => {};
  export let onBlur: () => void = () => {};
  export let onKeydown: (e: KeyboardEvent) => void = () => {};
  export let placeholder = '';
  /** Bindable — the live contenteditable DOM node, for `AutocompleteSuggest`'s
   *  `anchor` prop and imperative `.focus()` calls, mirroring exactly how
   *  `bind:this={goalInput}` worked against the plain `<textarea>` this
   *  replaces. */
  export let rootEl: HTMLDivElement | undefined = undefined;

  const CHIP_CLASS: Record<NonNullable<GoalSegment['chipKind']>, string> = {
    alias: 'chip-alias',
    repo: 'chip-repo',
    effort: 'chip-effort',
    model: 'chip-model',
    branch: 'chip-branch',
    autonomy: 'chip-autonomy',
    // Its own mint, distinct from `chip-autonomy`'s violet — the two used to
    // share one generic bucket, so `;eval/kcqf` rendered identically to an
    // autonomy override.
    eval: 'chip-eval',
    loop: 'chip-loop',
    // A real Claude Code `/name` command (Composer-Grammar-2) — deliberately
    // its own color, not a reuse of any `;`-verb hue: the brief's suggestion
    // to reuse the freed generic violet no longer applies once `chip-command`
    // became `chip-autonomy` (still actively claimed by six `;` commands,
    // never actually freed) — see LEDGER.md.
    claude: 'chip-claude'
  };

  /** What this component itself last emitted via `onInput` — the guard that
   *  tells a self-driven change (normal typing, leave the DOM alone) apart
   *  from an external one (a selection just landed in `value`, rebuild). */
  let lastEmitted: string | undefined;
  /** True once the very first build has happened — guards the initial mount
   *  out of `resync`'s auto-focus below. Every *later* external write
   *  (a selection, a post-commit reset to a fresh empty draft) already comes
   *  with its own deliberate `goalInput?.focus()` call at the existing
   *  parent call sites, same as before this component existed — but mount
   *  itself never did, and must not start doing so now (stealing focus onto
   *  every freshly-rendered pane's composer on page load would be a real
   *  regression, not a fix). */
  let hasMounted = false;

  function buildDom() {
    if (!rootEl) return;
    rootEl.textContent = '';
    for (const seg of segments) {
      if (seg.chipKind) {
        const span = document.createElement('span');
        span.className = `chip ${CHIP_CLASS[seg.chipKind]}`;
        span.contentEditable = 'false';
        span.textContent = seg.text;
        rootEl.appendChild(span);
      } else if (seg.text) {
        rootEl.appendChild(document.createTextNode(seg.text));
      }
    }
    // A trailing chip with nothing after it leaves no plain text node for the
    // caret to land in/after — an empty one gives typing somewhere sane to go.
    if (rootEl.lastChild && rootEl.lastChild.nodeType !== Node.TEXT_NODE) {
      rootEl.appendChild(document.createTextNode(''));
    }
  }

  function placeCaretAtEnd() {
    if (!rootEl) return;
    const range = document.createRange();
    range.selectNodeContents(rootEl);
    range.collapse(false);
    const sel = window.getSelection();
    sel?.removeAllRanges();
    sel?.addRange(range);
  }

  async function resync() {
    buildDom();
    lastEmitted = value;
    if (!hasMounted) {
      hasMounted = true;
      return;
    }
    await tick();
    // Unconditionally focus + place the caret at the end, not only when
    // already focused: an external write (a selection, a template drop) is
    // always immediately followed by the caller's own `goalInput?.focus()`
    // (unchanged from before this component existed), and racing two
    // independent `tick().then()` chains against a conditional focus-check
    // here would make which one "wins" — and therefore where the caret ends
    // up — nondeterministic. Doing it here unconditionally makes the result
    // the same either way: cursor lands right after the freshly-rendered
    // content, ready to keep typing.
    rootEl?.focus();
    placeCaretAtEnd();
  }

  // Rebuilds only on an externally-driven value change (see file doc) — the
  // `rootEl &&` guard defers the very first build until after mount, once
  // `bind:this` has actually populated it.
  $: if (rootEl && value !== lastEmitted) resync();

  function serialize(): string {
    if (!rootEl) return '';
    let out = '';
    for (const node of Array.from(rootEl.childNodes)) out += node.textContent ?? '';
    return out;
  }

  /** True when the caret sits at the very end of the field's content —
   *  the only place a rebuild-triggered `placeCaretAtEnd()` (see `resync`)
   *  is invisible rather than a jump. */
  function caretAtEnd(): boolean {
    if (!rootEl) return false;
    const sel = window.getSelection();
    if (!sel || sel.rangeCount === 0 || !sel.getRangeAt(0).collapsed) return false;
    const { endContainer, endOffset } = sel.getRangeAt(0);
    const last = rootEl.lastChild;
    // `buildDom` always leaves a (possibly empty) trailing text node — a
    // chip is never the last child on its own — so the end-of-field
    // boundary is always "end of that text node's content", not some
    // ancestor/descendant boundary-point pair `compareBoundaryPoints` would
    // need to reconcile (it doesn't treat "(parent, childCount)" and "(that
    // child's own text node, its length)" as equal, even though they're the
    // same visual caret position).
    if (!last) return true;
    if (last.nodeType === Node.TEXT_NODE) {
      return endContainer === last && endOffset === (last.textContent?.length ?? 0);
    }
    return endContainer === rootEl && endOffset === rootEl.childNodes.length;
  }

  function handleInput() {
    const next = serialize();
    // A trailing space right at the tail is the trigger to resolve a
    // just-completed token (`:alias`, `@repo`, `;cmd/value`, `xN`) into a
    // chip while still typing — mirrors mention-style editors (Slack/
    // Notion resolve on the delimiter, not on every keystroke). Resolving
    // eagerly on every keystroke instead would lock `x1` in as an atomic,
    // non-editable chip before the user finishes typing `x10` — there'd be
    // nowhere for the next digit to land. Leaving `lastEmitted` stale here
    // (rather than the usual `lastEmitted = next`) is what lets the
    // `resync()` guard below fire once `value` flows back through the
    // caller; gated on `caretAtEnd()` so a space typed mid-document (fixing
    // an earlier word) never jumps the caret away from where it actually
    // is.
    if (/\s$/.test(next) && caretAtEnd()) {
      onInput(next);
      return;
    }
    lastEmitted = next;
    onInput(next);
  }

  // Plain-text paste only — a pasted `<b>`/`<div>`-laden clipboard payload
  // would otherwise leave stray formatting nodes in a field that only ever
  // meant to hold one line of composer grammar.
  function handlePaste(e: ClipboardEvent) {
    e.preventDefault();
    const text = e.clipboardData?.getData('text/plain') ?? '';
    document.execCommand('insertText', false, text);
  }
</script>

<div
  class="chipinput"
  class:empty={!value}
  bind:this={rootEl}
  contenteditable="true"
  role="textbox"
  tabindex="0"
  aria-multiline="false"
  data-placeholder={placeholder}
  on:input={handleInput}
  on:keydown={onKeydown}
  on:paste={handlePaste}
  on:focus={onFocus}
  on:blur={onBlur}
  spellcheck="false"
></div>

<style>
  .chipinput {
    display: block;
    width: 100%;
    box-sizing: border-box;
    min-height: 1.5em;
    outline: none;
    white-space: pre-wrap;
    word-break: break-word;
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    line-height: 1.5;
  }
  .chipinput.empty::before {
    content: attr(data-placeholder);
    color: rgb(var(--k-text-primary-rgb) / 0.28);
    pointer-events: none;
  }
  .chipinput :global(.chip) {
    display: inline-flex;
    align-items: center;
    margin: 0 1px;
    padding: 0 6px;
    border-radius: 9px;
    font-size: 0.92em;
    font-weight: 700;
    white-space: nowrap;
    user-select: none;
  }
  .chipinput :global(.chip.chip-alias) {
    border: 1px solid rgb(var(--k-chip-alias-rgb) / 0.4);
    background: rgb(var(--k-chip-alias-rgb) / 0.1);
    color: var(--stack-teal, var(--k-chip-alias));
  }
  .chipinput :global(.chip.chip-repo) {
    border: 1px solid rgb(var(--k-chip-repo-rgb) / 0.4);
    background: rgb(var(--k-chip-repo-rgb) / 0.1);
    color: var(--konjo-ice, var(--k-chip-repo));
  }
  .chipinput :global(.chip.chip-effort) {
    border: 1px solid rgb(var(--k-chip-effort-rgb) / 0.4);
    background: rgb(var(--k-chip-effort-rgb) / 0.1);
    color: var(--konjo-sun, var(--k-chip-effort));
  }
  .chipinput :global(.chip.chip-model) {
    border: 1px solid rgb(var(--k-chip-model-rgb) / 0.4);
    background: rgb(var(--k-chip-model-rgb) / 0.1);
    color: var(--stack-violet, var(--k-chip-model));
  }
  .chipinput :global(.chip.chip-branch) {
    border: 1px solid rgb(var(--k-preset-benchmark-rgb) / 0.4);
    background: rgb(var(--k-preset-benchmark-rgb) / 0.1);
    color: rgb(var(--k-preset-benchmark-rgb));
  }
  .chipinput :global(.chip.chip-autonomy) {
    /* Flame orange — same accent as `.chip-loop`/the iteration pill. */
    border: 1px solid rgb(var(--k-chip-loop-rgb) / 0.4);
    background: rgb(var(--k-chip-loop-rgb) / 0.1);
    color: var(--konjo-flame, var(--k-chip-loop));
  }
  .chipinput :global(.chip.chip-eval) {
    border: 1px solid rgb(var(--k-ext-mint-rgb) / 0.4);
    background: rgb(var(--k-ext-mint-rgb) / 0.1);
    color: var(--konjo-mint, var(--k-ext-mint));
  }
  .chipinput :global(.chip.chip-loop) {
    border: 1px solid rgb(var(--k-chip-loop-rgb) / 0.4);
    background: rgb(var(--k-chip-loop-rgb) / 0.1);
    color: var(--konjo-flame, var(--k-chip-loop));
  }
  .chipinput :global(.chip.chip-claude) {
    border: 1px solid rgb(var(--k-danger-rgb) / 0.4);
    background: rgb(var(--k-danger-rgb) / 0.1);
    color: var(--konjo-rose, var(--k-danger));
  }
</style>
