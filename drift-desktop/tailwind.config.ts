import type { Config } from 'tailwindcss'
import { resolve } from 'path'

const config: Config = {
  content: [
    resolve(__dirname, '../drift-ui/src/**/*.{ts,tsx}'),
    resolve(__dirname, 'src/renderer/**/*.{ts,tsx}'),
  ],
  theme: {
    extend: {
      fontFamily: {
        sans:  ['IBM Plex Sans', 'system-ui', 'sans-serif'],
        head:  ['Space Grotesk', 'sans-serif'],
        mono:  ['JetBrains Mono', 'IBM Plex Mono', 'monospace'],
      },
    },
  },
  plugins: [],
}

export default config
