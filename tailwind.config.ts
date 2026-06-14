import type { Config } from 'tailwindcss'

export default {
  content: ['./src/renderer/**/*.{ts,tsx,html}'],
  theme: {
    extend: {
      fontFamily: {
        sans: ['Geist', '-apple-system', 'BlinkMacSystemFont', 'Segoe UI', 'sans-serif'],
        display: ['Geist', '-apple-system', 'BlinkMacSystemFont', 'Segoe UI', 'sans-serif'],
        mono: ['"Geist Mono"', 'ui-monospace', 'SFMono-Regular', 'Menlo', 'monospace'],
      },
      borderRadius: {
        'tezbar-chip': 'var(--r-chip)',
        'tezbar-field': 'var(--r-field)',
        'tezbar-row': 'var(--r-row)',
        'tezbar-card': 'var(--r-card)',
        'tezbar-panel': 'var(--r-panel)',
        'tezbar-window': 'var(--r-window)',
      },
      boxShadow: {
        'tezbar-sm': 'none',
        'tezbar-md': 'none',
        'tezbar-lg': 'none',
        'tezbar-glow': 'none',
      },
      colors: {
        glass: {
          shell: 'rgb(var(--c-shell) / <alpha-value>)',
          panel: 'rgb(var(--c-panel) / <alpha-value>)',
          row: 'rgb(var(--c-row) / <alpha-value>)',
        },
        ink: {
          1: 'rgb(var(--c-text-1) / <alpha-value>)',
          2: 'rgb(var(--c-text-2) / <alpha-value>)',
          3: 'rgb(var(--c-text-3) / <alpha-value>)',
          4: 'rgb(var(--c-text-4) / <alpha-value>)',
        },
        accent: {
          DEFAULT: 'rgb(var(--c-accent) / <alpha-value>)',
          strong: 'rgb(var(--c-accent-strong) / <alpha-value>)',
        },
      },
      transitionTimingFunction: {
        'out-expo': 'cubic-bezier(0.22, 1, 0.36, 1)',
        spring: 'cubic-bezier(0.34, 1.56, 0.64, 1)',
      },
      transitionDuration: {
        fast: '140ms',
        base: '220ms',
        slow: '360ms',
      },
      keyframes: {
        'tezbar-fade-up': {
          '0%': { opacity: '0', transform: 'translateY(4px)' },
          '100%': { opacity: '1', transform: 'translateY(0)' },
        },
        'tezbar-fade-in': {
          '0%': { opacity: '0' },
          '100%': { opacity: '1' },
        },
        'tezbar-scale-in': {
          '0%': { opacity: '0', transform: 'scale(0.96)' },
          '100%': { opacity: '1', transform: 'scale(1)' },
        },
        'tezbar-shimmer': {
          '0%': { backgroundPosition: '-200% 0' },
          '100%': { backgroundPosition: '200% 0' },
        },
      },
      animation: {
        'tezbar-fade-up': 'tezbar-fade-up 360ms cubic-bezier(0.22, 1, 0.36, 1) both',
        'tezbar-fade-in': 'tezbar-fade-in 90ms cubic-bezier(0.22, 1, 0.36, 1) both',
        'tezbar-scale-in': 'tezbar-scale-in 260ms cubic-bezier(0.22, 1, 0.36, 1) both',
        'tezbar-shimmer': 'tezbar-shimmer 2.2s linear infinite',
      },
    },
  },
  plugins: [],
} satisfies Config
