import type { Config } from 'tailwindcss'

const config: Config = {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        'bg-base': '#0f0f14',
        cobalt: '#3805e3',
        'text-dim': '#8b8fa8',
        'drift-red': '#f04438',
        'drift-amber': '#f79009',
        'drift-teal': '#0d9488',
      },
    },
  },
  plugins: [],
}

export default config
