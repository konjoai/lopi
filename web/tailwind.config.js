/** @type {import('tailwindcss').Config} */
export default {
  content: ['./src/**/*.{html,js,svelte,ts}'],
  theme: {
    extend: {
      colors: {
        konjo: {
          black: '#0a0a0a',
          deep: '#050505',
          paper: '#f5f5f5',
          ice: '#00d4ff',
          'ice-deep': '#0088aa',
          ember: '#ff4500',
          flame: '#ff9500',
          jade: '#00ff9d',
          sun: '#ffcc00',
          rose: '#ff0066',
          mist: 'rgba(255,255,255,0.04)',
          veil: 'rgba(255,255,255,0.08)',
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
