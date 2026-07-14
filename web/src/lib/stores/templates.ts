/**
 * Template persistence (Creation-Flow-1 §2) — a Svelte store backed by
 * **localStorage** under `lopi.templates.v1`.
 *
 * CLIENT-ONLY, EXPLICITLY NOT DURABLE. There is no backend and no sync: these
 * templates live in one browser profile and are lost if the user clears site
 * data or opens another machine/browser. Cross-machine sharing is deliberately
 * out of scope (see CHANGELOG / NEXT_SESSION_PROMPT). Every localStorage access
 * is wrapped in try/catch so private-mode, a full quota, or corrupt JSON
 * degrades to an empty in-memory set rather than throwing.
 */
import { writable } from 'svelte/store';
import type { PromptTemplate, StackTemplate } from './stack';

/** The persisted shape under `lopi.templates.v1`. */
export interface TemplateStore {
  prompts: PromptTemplate[];
  stacks: StackTemplate[];
}

const STORAGE_KEY = 'lopi.templates.v1';

/** Seed templates written only when the key is absent (first run) — a couple
 *  of starting points so the dropdown isn't empty on a fresh profile. Ids are
 *  static (seeds, not minted) so a re-seed can't collide with user templates. */
function seedTemplates(): TemplateStore {
  return {
    prompts: [
      { id: 'seed-prompt-research', name: 'deep research', preset: 'research', goal: 'investigate the problem space and summarize findings' },
      { id: 'seed-prompt-implement', name: 'ship a feature', preset: 'implement', goal: 'implement the change end-to-end with tests' }
    ],
    stacks: [
      {
        id: 'seed-stack-kcqf',
        name: 'kcqf sprint',
        // Serialized bottom-first (run order): research runs first, then
        // implement, then optimize — `applyStackTemplate` lands research at the
        // bottom of the pane.
        loops: [
          { preset: 'research', goal: 'research the problem space' },
          { preset: 'implement', goal: 'implement the change' },
          { preset: 'optimize', goal: 'optimize and harden' }
        ]
      }
    ]
  };
}

/** An empty set — the safe fallback for every read failure. */
function emptyTemplates(): TemplateStore {
  return { prompts: [], stacks: [] };
}

/** Read the persisted templates, or fall back. Seeds on first run (absent
 *  key). Never throws — any storage/parse failure yields an empty set. */
function load(): TemplateStore {
  try {
    if (typeof localStorage === 'undefined') return emptyTemplates();
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw === null) {
      const seeded = seedTemplates();
      persist(seeded);
      return seeded;
    }
    const parsed = JSON.parse(raw) as Partial<TemplateStore>;
    return {
      prompts: Array.isArray(parsed?.prompts) ? parsed.prompts : [],
      stacks: Array.isArray(parsed?.stacks) ? parsed.stacks : []
    };
  } catch {
    // Private mode / corrupt JSON / quota read error — degrade to empty.
    return emptyTemplates();
  }
}

/** Best-effort write. Swallows quota/private-mode failures (logged) — the
 *  in-memory store stays authoritative for the session either way. */
function persist(value: TemplateStore): void {
  try {
    if (typeof localStorage === 'undefined') return;
    localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
  } catch (err) {
    // No silent failure: surface why the template didn't persist, but never
    // throw into the caller's click handler.
    console.warn('lopi: could not persist templates (client-only, not durable):', err);
  }
}

/** The live template set. Client-only, in-memory-authoritative, mirrored to
 *  localStorage on every mutation via the helpers below. */
export const templates = writable<TemplateStore>(load());

/** Append a prompt template and persist. */
export function savePromptTemplate(tpl: PromptTemplate): void {
  templates.update((cur) => {
    const next = { ...cur, prompts: [...cur.prompts, tpl] };
    persist(next);
    return next;
  });
}

/** Append a stack template and persist. */
export function saveStackTemplate(tpl: StackTemplate): void {
  templates.update((cur) => {
    const next = { ...cur, stacks: [...cur.stacks, tpl] };
    persist(next);
    return next;
  });
}
