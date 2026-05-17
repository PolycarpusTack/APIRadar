import { useState, useCallback } from 'react'
import { Telescope } from 'lucide-react'
import PageHeader from '../components/PageHeader'

const DEFAULT_SPEC = 'https://cdn.jsdelivr.net/npm/@scalar/galaxy/dist/latest.yaml'

function buildScalarHtml(specUrl: string, theme: 'dark' | 'light') {
  return `<!doctype html>
<html>
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>API Playground</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { background: ${theme === 'dark' ? '#0B0F19' : '#ffffff'}; }
  </style>
</head>
<body>
  <script
    id="api-reference"
    data-url="${specUrl}"
    data-configuration='${JSON.stringify({
      theme: theme === 'dark' ? 'saturn' : 'default',
      darkMode: theme === 'dark',
      hideClientButton: false,
      showSidebar: true,
    })}'
  ></script>
  <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
</body>
</html>`
}

export default function PlaygroundPage() {
  const [inputUrl, setInputUrl] = useState(DEFAULT_SPEC)
  const [activeUrl, setActiveUrl] = useState(DEFAULT_SPEC)
  const [theme] = useState<'dark' | 'light'>('dark')

  const handleLoad = useCallback(() => {
    const trimmed = inputUrl.trim()
    if (trimmed) setActiveUrl(trimmed)
  }, [inputUrl])

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key === 'Enter') handleLoad()
    },
    [handleLoad],
  )

  return (
    <div className="flex flex-col" style={{ height: '100vh' }}>
      <PageHeader
        tag="Playground"
        title="API Explorer"
        description="Interactive API playground powered by Scalar. Enter any OpenAPI spec URL to try endpoints live — no Postman required."
      />

      {/* URL bar */}
      <div
        className="flex items-center gap-3 px-14 py-4 flex-shrink-0"
        style={{ background: 'var(--bg-surface)', borderBottom: '1px solid var(--border)' }}
      >
        <Telescope className="h-4 w-4 flex-shrink-0" style={{ color: 'var(--text-3)' }} />
        <input
          type="url"
          value={inputUrl}
          onChange={(e) => setInputUrl(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="https://api.example.com/openapi.yaml"
          className="flex-1 rounded-md border px-3 py-[7px] text-[12.5px] outline-none transition-colors"
          style={{
            background: 'var(--bg-raised)',
            border: '1px solid var(--border)',
            color: 'var(--text-1)',
            fontFamily: 'var(--font-mono)',
          }}
          onFocus={(e) => {
            e.currentTarget.style.borderColor = 'var(--cobalt)'
            e.currentTarget.style.boxShadow = '0 0 0 3px rgba(56,5,227,0.15)'
          }}
          onBlur={(e) => {
            e.currentTarget.style.borderColor = 'var(--border)'
            e.currentTarget.style.boxShadow = ''
          }}
        />
        <button
          onClick={handleLoad}
          className="flex-shrink-0 rounded-md px-4 py-[7px] text-[12.5px] font-semibold transition-all"
          style={{
            background: 'var(--cobalt)',
            color: '#fff',
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.background = 'var(--cobalt-mid)'
            e.currentTarget.style.transform = 'translateY(-1px)'
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.background = 'var(--cobalt)'
            e.currentTarget.style.transform = ''
          }}
        >
          Load Spec
        </button>
      </div>

      {/* Scalar iframe — fills remaining height */}
      <div className="flex-1 overflow-hidden">
        <iframe
          key={activeUrl}
          srcDoc={buildScalarHtml(activeUrl, theme)}
          title="API Playground"
          sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
          style={{ width: '100%', height: '100%', border: 'none' }}
        />
      </div>
    </div>
  )
}
