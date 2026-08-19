/** @type {import('tailwindcss').Config} */
export default {
  content: ['./src/**/*.{html,js,svelte,ts}'],
  theme: {
    extend: {
      colors: {
        konjo: {
          black: 'rgb(var(--k-ext-surface-black-rgb) / <alpha-value>)',
          deep: 'rgb(var(--k-surface-base-rgb) / <alpha-value>)',
          paper: 'rgb(var(--k-text-primary-rgb) / <alpha-value>)',
          ice: 'rgb(var(--k-chip-repo-rgb) / <alpha-value>)',
          'ice-deep': 'rgb(var(--k-ext-ice-deep-rgb) / <alpha-value>)',
          ember: 'rgb(var(--k-ext-ember-rgb) / <alpha-value>)',
          flame: 'rgb(var(--k-chip-loop-rgb) / <alpha-value>)',
          jade: 'rgb(var(--k-preset-benchmark-rgb) / <alpha-value>)',
          sun: 'rgb(var(--k-chip-effort-rgb) / <alpha-value>)',
          rose: 'rgb(var(--k-danger-rgb) / <alpha-value>)',
          // Budget page (Phase 10 redesign) — a brighter teal than `ice` and a
          // lighter violet than the existing phase-testing `violet`, plus the
          // slightly-darker-than-`deep` card background the notch-badge stat
          // cards sit on. Distinct keys so they don't recolor existing usages.
          teal: 'rgb(var(--k-chip-alias-rgb) / <alpha-value>)',
          'violet-light': 'rgb(var(--k-chip-model-rgb) / <alpha-value>)',
          card: 'rgb(var(--k-ext-surface-panel-rgb) / <alpha-value>)',
          mist: 'rgb(var(--k-wash-rgb) / 0.04)',
          veil: 'rgb(var(--k-wash-rgb) / 0.08)',
          accent: 'rgb(var(--konjo-accent-rgb) / <alpha-value>)'
        }
      },
      fontFamily: {
        sans: ['Inter', 'system-ui', 'sans-serif'],
        mono: ['JetBrains Mono', 'ui-monospace', 'monospace']
      },
      animation: {
        'pulse-slow': 'pulse 3s cubic-bezier(0.4, 0, 0.6, 1) infinite',
        'breathe': 'breathe 4s ease-in-out infinite',
        'flicker': 'flicker 2s ease-in-out infinite'
      },
      keyframes: {
        breathe: {
          '0%, 100%': { opacity: '0.6', transform: 'scale(1)' },
          '50%': { opacity: '1', transform: 'scale(1.02)' }
        },
        flicker: {
          '0%, 100%': { opacity: '1' },
          '50%': { opacity: '0.85' }
        }
      }
    }
  },
  plugins: []
};
