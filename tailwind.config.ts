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
        'raymes-chip': 'var(--r-chip)',
        'raymes-field': 'var(--r-field)',
        'raymes-row': 'var(--r-row)',
        'raymes-card': 'var(--r-card)',
        'raymes-panel': 'var(--r-panel)',
        'raymes-window': 'var(--r-window)',
      },
      boxShadow: {
        'raymes-sm': 'none',
        'raymes-md': 'none',
        'raymes-lg': 'none',
        'raymes-glow': 'none',
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
        'raymes-fade-up': {
          '0%': { opacity: '0', transform: 'translateY(4px)' },
          '100%': { opacity: '1', transform: 'translateY(0)' },
        },
        'raymes-fade-in': {
          '0%': { opacity: '0' },
          '100%': { opacity: '1' },
        },
        'raymes-scale-in': {
          '0%': { opacity: '0', transform: 'scale(0.96)' },
          '100%': { opacity: '1', transform: 'scale(1)' },
        },
        'raymes-shimmer': {
          '0%': { backgroundPosition: '-200% 0' },
          '100%': { backgroundPosition: '200% 0' },
        },
      },
      animation: {
        'raymes-fade-up': 'raymes-fade-up 360ms cubic-bezier(0.22, 1, 0.36, 1) both',
        'raymes-fade-in': 'raymes-fade-in 90ms cubic-bezier(0.22, 1, 0.36, 1) both',
        'raymes-scale-in': 'raymes-scale-in 260ms cubic-bezier(0.22, 1, 0.36, 1) both',
        'raymes-shimmer': 'raymes-shimmer 2.2s linear infinite',
      },
    },
  },
  plugins: [],
} satisfies Config
