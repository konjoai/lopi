import { redirect } from '@sveltejs/kit';

/**
 * Shell-1: Loop Stacks is the app's default view. `/` (Forge's old home)
 * redirects here instead of Forge's page moving into the root route —
 * reversible (delete this file to restore Forge at `/`) and leaves the
 * `/stacks` route folder itself completely untouched. Forge moved to
 * `/forge`, still reachable from the sidebar like every other destination.
 */
export function load() {
  throw redirect(307, '/stacks');
}
