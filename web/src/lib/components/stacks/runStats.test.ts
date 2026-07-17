/**
 * Pure formatter tests for `RunStatsPill.svelte` — run with
 * `npx tsx src/lib/components/stacks/runStats.test.ts`.
 */
import { formatElapsed, formatTokens, formatCost } from './runStats';
import { eq, namedSummary } from '$lib/test-harness';

// ── formatElapsed ──────────────────────────────────────────────────────────
eq(formatElapsed(0), '0s', '0ms → 0s');
eq(formatElapsed(42_000), '42s', 'under a minute → seconds only');
eq(formatElapsed(4 * 60_000 + 27_000), '4m 27s', 'reference case: 4m 27s');
eq(formatElapsed(60_000), '1m 00s', 'exactly one minute pads seconds');
eq(formatElapsed(3661_000), '1h 01m', 'over an hour drops to hours + minutes, no seconds');
eq(formatElapsed(-500), '0s', 'negative elapsed clamps to 0s, never a negative label');

// ── formatTokens ────────────────────────────────────────────────────────────
eq(formatTokens(0), '0', 'zero tokens');
eq(formatTokens(840), '840', 'under 1k stays a raw integer');
eq(formatTokens(3_400), '3.4k', 'reference case: 3.4k');
eq(formatTokens(999), '999', 'just under the k threshold stays raw');
eq(formatTokens(1_200_000), '1.2m', 'millions get their own suffix');

// ── formatCost ────────────────────────────────────────────────────────────
eq(formatCost(0), '$0.00', 'zero cost');
eq(formatCost(0.004), '<$0.01', 'sub-cent spend reads as a floor, not $0.00');
eq(formatCost(0.42), '$0.42', 'ordinary cost at cent precision');
eq(formatCost(-0.01), '$0.00', 'a negative cost (should never happen) never renders negative');

namedSummary('runStats');
