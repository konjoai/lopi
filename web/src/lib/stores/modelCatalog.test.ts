import { modelOptionsFrom, effortOptionsFor, type ModelCatalogEntry } from './modelCatalog';
import { MODEL_OPTIONS, EFFORT_OPTIONS } from './options';
import { eq, eqIs, ok, namedSummary } from '$lib/test-harness';

const catalog: ModelCatalogEntry[] = [
  { id: 'claude-opus-4-9', display_name: 'Opus 4.9', effort: ['low', 'medium', 'high', 'xhigh', 'max'] },
  { id: 'claude-haiku-5', display_name: 'Haiku 5', effort: ['low', 'medium'] }
];

// ── modelOptionsFrom ──────────────────────────────────────────────────────────
eq(modelOptionsFrom([]), MODEL_OPTIONS, 'an empty (unloaded) catalog falls back to the static list');
{
  const options = modelOptionsFrom(catalog);
  eqIs(options.length, 3, 'a loaded catalog is every live model plus the auto sentinel');
  eqIs(options[0].value, 'claude-opus-4-9', 'live models come first, in catalog order');
  eqIs(options[0].label, 'Opus 4.9', 'the label is the catalog display_name');
  eqIs(options[1].value, 'claude-haiku-5', 'every catalog entry gets an option');
  eqIs(options[2].value, 'auto', 'auto is appended last, sourced from the static list');
  eqIs(options[2].label, 'Auto', 'auto keeps its static label');
}

// ── effortOptionsFor ──────────────────────────────────────────────────────────
eq(effortOptionsFor([], 'claude-opus-4-9'), EFFORT_OPTIONS, 'an empty (unloaded) catalog falls back to the static list');
eq(
  effortOptionsFor(catalog, 'claude-sonnet-not-in-catalog'),
  EFFORT_OPTIONS,
  'a model absent from the catalog (e.g. `auto`) falls back to the static list'
);
{
  const options = effortOptionsFor(catalog, 'claude-opus-4-9');
  eqIs(options.length, 5, 'every tier the model supports gets an option, no more');
  eqIs(options[0].value, 'low', 'tier order is preserved from the catalog');
  eqIs(options[0].label, 'Low', "a tier matching a static EFFORT_OPTIONS value reuses its label ('Low', not 'low')");
  ok(options[0].hint !== undefined, 'a known tier also reuses the static hint');
  eqIs(options[3].value, 'xhigh', 'an unknown tier (not in the static EFFORT_OPTIONS) still appears');
  eqIs(options[3].label, 'Xhigh', 'an unknown tier gets a capitalized fallback label');
}
{
  const options = effortOptionsFor(catalog, 'claude-haiku-5');
  eqIs(options.length, 2, "a model with fewer tiers than another model's is not padded out to match");
  eqIs(
    options.map((o) => o.value).join(','),
    'low,medium',
    'haiku only offers the tiers it actually supports'
  );
}

namedSummary('modelCatalog');
