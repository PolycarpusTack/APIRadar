import type { Config } from 'tailwindcss'

const config: Config = {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      fontFamily: {
        sans:  ['IBM Plex Sans', 'system-ui', 'sans-serif'],
        head:  ['Space Grotesk', 'sans-serif'],
        mono:  ['JetBrains Mono', 'IBM Plex Mono', 'monospace'],
      },
      colors: {
        'bg-base':    'var(--bg-base)',
        'bg-surface': 'var(--bg-surface)',
        'bg-raised':  'var(--bg-raised)',
        'bg-hover':   'var(--bg-hover)',
        'bg-active':  'var(--bg-active)',
        cobalt:       'var(--cobalt)',
        'cobalt-mid': 'var(--cobalt-mid)',
        'neon-green': 'var(--neon-green)',
        'ui-red':     'var(--red)',
        'ui-amber':   'var(--amber)',
        'ui-teal':    'var(--teal)',
        'ui-blue':    'var(--blue)',
        'ui-purple':  'var(--purple)',
        'text-1':     'var(--text-1)',
        'text-2':     'var(--text-2)',
        'text-3':     'var(--text-3)',
        'text-dim':   'var(--text-dim)',
        'ui-border':  'var(--border)',
        'border-mid': 'var(--border-mid)',
        'border-hi':  'var(--border-hi)',
      },
      borderRadius: {
        sm:   'var(--radius-sm)',
        md:   'var(--radius-md)',
        lg:   'var(--radius-lg)',
        xl:   'var(--radius-xl)',
        full: 'var(--radius-full)',
      },
    },
  },
  plugins: [],
}

export default config
