<!--
  HelpOverlay — keyboard shortcut reference, toggled via ?.
  Esc dismisses. Click outside dismisses.
-->
<script lang="ts">
  import { helpVisible } from '$lib/stores/keyboard';

  function dismiss() {
    helpVisible.set(false);
  }

  const shortcuts: { keys: string[]; label: string }[] = [
    { keys: ['j', '↓'], label: 'next agent' },
    { keys: ['k', '↑'], label: 'previous agent' },
    { keys: ['⌘', 'K'], label: 'toggle Forge ↔ Constellation' },
    { keys: ['Esc'], label: 'clear focus / dismiss' },
    { keys: ['?'], label: 'show this overlay' }
  ];
</script>

{#if $helpVisible}
  <div
    class="fixed inset-0 z-40 flex items-center justify-center bg-black/70 backdrop-blur-sm"
    on:click={dismiss}
    on:keydown={(e) => e.key === 'Escape' && dismiss()}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <div
      class="bg-konjo-deep border border-white/10 rounded-2xl px-8 py-7 max-w-sm w-full mx-4 shadow-2xl"
      on:click|stopPropagation
      on:keydown|stopPropagation
      role="presentation"
    >
      <div class="font-display text-xl mb-1">Keyboard</div>
      <div class="font-mono text-[10px] uppercase tracking-widest opacity-50 mb-5">
        keep your hands on the keyboard
      </div>

      <div class="space-y-3">
        {#each shortcuts as s}
          <div class="flex items-center justify-between text-sm">
            <span class="opacity-80">{s.label}</span>
            <span class="flex gap-1">
              {#each s.keys as key}
                <kbd
                  class="font-mono text-[11px] px-2 py-0.5 rounded bg-white/5 border border-white/10 text-konjo-ice tabular-nums"
                >
                  {key}
                </kbd>
              {/each}
            </span>
          </div>
        {/each}
      </div>

      <div class="mt-6 pt-4 border-t border-white/5 font-mono text-[10px] uppercase tracking-widest opacity-40 text-center">
        press ? again to dismiss
      </div>
    </div>
  </div>
{/if}
