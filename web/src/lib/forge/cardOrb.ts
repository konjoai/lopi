/**
 * cardOrb — resolve a stack card's live orb state from the shared `agents`
 * store, keyed by the card's `taskId`.
 *
 * This is the one place the card→orb lookup lives, kept pure (no store
 * subscription, no `$app` imports) so a `StackCard` and the WebGL orb renderer
 * (`ForgeStage`/`orbState`) provably speak the same status vocabulary: both
 * funnel the same `AgentState` through the same `computeOrbState`, so the orb a
 * card shows for a live agent is byte-for-byte the orb the full renderer would.
 * The only difference is the host component. A card with no `taskId` (never
 * launched) resolves to the calm idle orb, exactly like an empty pane.
 */
import type { AgentState } from '$lib/stores/agents';
import { computeOrbState, IDLE_ORB, type OrbState } from './orbState';

/**
 * The orb state for one stack card. `taskId` is `card.taskId` (set once the
 * card is submitted as a real task); `agents` is the live `agents` store value;
 * `waiting` is the `permissionWaiting` set (agent ids paused for a human).
 * Returns {@link IDLE_ORB} when the card carries no task or its task is not (yet)
 * in the store; otherwise the same {@link computeOrbState} result `AgentPane`
 * would render for that agent.
 */
export function orbStateForCard(
  taskId: string | undefined,
  agents: Map<string, AgentState>,
  waiting: ReadonlySet<string>
): OrbState {
  if (!taskId) return IDLE_ORB;
  const agent = agents.get(taskId) ?? null;
  if (!agent) return IDLE_ORB;
  return computeOrbState(agent, waiting.has(agent.id));
}
