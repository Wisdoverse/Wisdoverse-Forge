import type { Config } from 'tailwindcss'

const config: Config = {
  darkMode: 'class',
  content: ['./src/app/**/*.{ts,tsx}', './index.html'],
  theme: {
    extend: {
      colors: {
        apple: {
          blue: '#0066CC',
          'blue-focus': '#0071E3',
          'blue-dark': '#2997FF',
          green: '#30D158',
          orange: '#FF9F0A',
          red: '#FF453A',
          purple: '#5856D6',
          gray: {
            1: '#8E8E93',
            2: '#AEAEB2',
            3: '#C7C7CC',
            4: '#D1D1D6',
            5: '#E5E5EA',
            6: '#F2F2F7',
          },
        },
        surface: {
          DEFAULT: 'rgba(245, 245, 247, 0.80)',
          elevated: 'rgba(255, 255, 255, 0.92)',
          light: '#FFFFFF',
          pearl: '#FAFAFC',
          parchment: '#F5F5F7',
        },
        'surface-dark': {
          DEFAULT: '#232326',
          elevated: '#2A2A2C',
        },
        background: {
          light: '#F7F7F8',
          dark: '#1C1C1E',
        },
        foreground: {
          light: '#1D1D1F',
          dark: '#F5F5F7',
        },
        secondary: {
          light: '#86868B',
          dark: '#98989D',
        },
      },
      borderRadius: {
        card: '8px',
        panel: '8px',
        button: '6px',
        badge: '9999px',
      },
      fontFamily: {
        sans: [
          'system-ui',
          '-apple-system',
          'BlinkMacSystemFont',
          'SF Pro Display',
          'SF Pro Text',
          'Inter',
          'Helvetica Neue',
          'Arial',
          'sans-serif',
        ],
        mono: ['SF Mono', 'Menlo', 'Monaco', 'Consolas', 'monospace'],
      },
      fontSize: {
        'ui-doc-title': ['24px', { lineHeight: '30px', letterSpacing: '-0.01em' }],
        'ui-title': ['15px', { lineHeight: '20px', letterSpacing: '0' }],
        'ui-section': ['14px', { lineHeight: '20px', letterSpacing: '0' }],
        'ui-body': ['13px', { lineHeight: '18px', letterSpacing: '0' }],
        'ui-button': ['13px', { lineHeight: '16px', letterSpacing: '0' }],
        'ui-caption': ['12px', { lineHeight: '16px', letterSpacing: '0' }],
        'ui-metric': ['20px', { lineHeight: '24px', letterSpacing: '0' }],
      },
      boxShadow: {
        card: 'none',
        panel: 'none',
        'card-dark': 'none',
        'panel-dark': 'none',
        product: '3px 5px 30px rgba(0, 0, 0, 0.22)',
      },
    },
  },
  plugins: [],
}

export default config
