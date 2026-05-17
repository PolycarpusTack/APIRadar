import { useState, useCallback } from 'react'
import { Telescope, ChevronDown, ChevronUp, Plus, Trash2 } from 'lucide-react'
import PageHeader from '../components/PageHeader'

const DEFAULT_SPEC = 'https://cdn.jsdelivr.net/npm/@scalar/galaxy/dist/latest.yaml'
const STORAGE_KEY = 'drift-playground-envs'
// Mirrors --bg-base token; used inside iframe srcdoc where CSS variables from the parent page are unavailable.
const BG_BASE_DARK = '#0B0F19'

interface PlayEnv {
  id: string
  name: string
  baseUrl: string
  bearerToken: string
}

interface EnvStore {
  envs: PlayEnv[]
  activeId: string | null
}

function loadStore(): EnvStore {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) return JSON.parse(raw) as EnvStore
  } catch {}
  return { envs: [], activeId: null }
}

function buildScalarHtml(specUrl: string, theme: 'dark' | 'light', env?: PlayEnv | null) {
  const config: Record<string, unknown> = {
    theme: theme === 'dark' ? 'saturn' : 'default',
    darkMode: theme === 'dark',
    hideClientButton: false,
    showSidebar: true,
  }
  if (env?.baseUrl) {
    config.servers = [{ url: env.baseUrl, description: env.name }]
  }
  if (env?.bearerToken) {
    config.authentication = { http: { bearer: { token: env.bearerToken } } }
  }
  return `<!doctype html>
<html>
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>API Playground</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { background: ${theme === 'dark' ? BG_BASE_DARK : '#ffffff'}; }
  </style>
</head>
<body>
  <script
    id="api-reference"
    data-url="${specUrl}"
    data-configuration='${JSON.stringify(config)}'
  ></script>
  <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
</body>
</html>`
}

const INPUT_STYLE = {
  background: 'var(--bg-raised)',
  border: '1px solid var(--border)',
  color: 'var(--text-1)',
  fontFamily: 'var(--font-mono)',
} as const

function focusInput(e: React.FocusEvent<HTMLInputElement>) {
  e.currentTarget.style.borderColor = 'var(--cobalt)'
  e.currentTarget.style.boxShadow = '0 0 0 3px rgba(56,5,227,0.15)'
}

function blurInput(e: React.FocusEvent<HTMLInputElement>) {
  e.currentTarget.style.borderColor = 'var(--border)'
  e.currentTarget.style.boxShadow = ''
}

export default function PlaygroundPage() {
  const [inputUrl, setInputUrl] = useState(DEFAULT_SPEC)
  const [activeUrl, setActiveUrl] = useState(DEFAULT_SPEC)
  const [theme] = useState<'dark' | 'light'>('dark')
  const [store, setStore] = useState<EnvStore>(loadStore)
  const [envOpen, setEnvOpen] = useState(false)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [form, setForm] = useState({ name: '', baseUrl: '', bearerToken: '' })
  const [isNewEnv, setIsNewEnv] = useState(false)

  const activeEnv = store.envs.find((e) => e.id === store.activeId) ?? null

  function persistStore(next: EnvStore) {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(next))
    setStore(next)
  }

  function selectEnvForEdit(env: PlayEnv | null) {
    if (env) {
      setSelectedId(env.id)
      setForm({ name: env.name, baseUrl: env.baseUrl, bearerToken: env.bearerToken })
      setIsNewEnv(false)
    } else {
      setSelectedId(null)
      setForm({ name: '', baseUrl: '', bearerToken: '' })
      setIsNewEnv(true)
    }
  }

  function activateEnv(id: string | null) {
    persistStore({ ...store, activeId: id })
  }

  function saveForm() {
    if (!form.name.trim()) return
    if (isNewEnv) {
      const newEnv: PlayEnv = {
        id: crypto.randomUUID(),
        name: form.name.trim(),
        baseUrl: form.baseUrl.trim(),
        bearerToken: form.bearerToken.trim(),
      }
      persistStore({ envs: [...store.envs, newEnv], activeId: newEnv.id })
      setSelectedId(newEnv.id)
      setIsNewEnv(false)
    } else if (selectedId) {
      const updated = store.envs.map((e) =>
        e.id === selectedId
          ? { ...e, name: form.name.trim(), baseUrl: form.baseUrl.trim(), bearerToken: form.bearerToken.trim() }
          : e,
      )
      persistStore({ ...store, envs: updated })
    }
  }

  function deleteSelected() {
    if (!selectedId) return
    const next = store.envs.filter((e) => e.id !== selectedId)
    persistStore({ envs: next, activeId: store.activeId === selectedId ? null : store.activeId })
    setSelectedId(null)
    setForm({ name: '', baseUrl: '', bearerToken: '' })
    setIsNewEnv(false)
  }

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

  const iframeKey = `${activeUrl}::${store.activeId ?? 'none'}`

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
          style={INPUT_STYLE}
          onFocus={focusInput}
          onBlur={blurInput}
        />
        <button
          onClick={handleLoad}
          className="flex-shrink-0 rounded-md px-4 py-[7px] text-[12.5px] font-semibold transition-all"
          style={{ background: 'var(--cobalt)', color: 'var(--text-inverse)' }}
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

      {/* Environment bar */}
      <div
        className="flex-shrink-0"
        style={{ background: 'var(--bg-raised)', borderBottom: '1px solid var(--border)' }}
      >
        {/* Toggle row */}
        <div className="flex items-center gap-3 px-14 py-2.5">
          <button
            onClick={() => setEnvOpen((o) => !o)}
            className="flex items-center gap-1.5 text-[11.5px] font-semibold uppercase tracking-[0.8px] transition-colors hover:text-[var(--text-2)]"
            style={{ color: 'var(--text-3)', fontFamily: 'var(--font-mono)' }}
          >
            {envOpen ? <ChevronUp className="h-3 w-3" /> : <ChevronDown className="h-3 w-3" />}
            Environments
          </button>
          {!envOpen && activeEnv && (
            <span
              className="rounded px-2 py-0.5 text-[11px] font-medium"
              style={{ background: 'var(--cobalt)', color: 'var(--text-inverse)', fontFamily: 'var(--font-mono)' }}
            >
              {activeEnv.name}
            </span>
          )}
          {!envOpen && !activeEnv && (
            <span className="text-[11px]" style={{ color: 'var(--text-dim)', fontFamily: 'var(--font-mono)' }}>
              none active
            </span>
          )}
        </div>

        {/* Expanded panel */}
        {envOpen && (
          <div className="px-14 pb-5 pt-1 grid gap-8" style={{ gridTemplateColumns: '1fr 1fr' }}>
            {/* Left: env list */}
            <div>
              <p className="mb-2 text-[9.5px] uppercase tracking-[1px] font-semibold" style={{ color: 'var(--text-dim)' }}>
                Saved Environments
              </p>
              <div className="flex flex-wrap gap-2 mb-3">
                {store.envs.map((env) => {
                  const isActive = env.id === store.activeId
                  const isSelected = env.id === selectedId
                  return (
                    <button
                      key={env.id}
                      onClick={() => {
                        activateEnv(env.id)
                        selectEnvForEdit(env)
                      }}
                      className="rounded-md px-3 py-1 text-[11.5px] font-medium transition-all"
                      style={{
                        background: isActive ? 'var(--cobalt)' : isSelected ? 'var(--bg-active)' : 'var(--bg-surface)',
                        border: `1px solid ${isActive ? 'var(--cobalt)' : isSelected ? 'var(--cobalt-mid)' : 'var(--border)'}`,
                        color: isActive ? '#fff' : 'var(--text-2)',
                        fontFamily: 'var(--font-mono)',
                      }}
                    >
                      {env.name}
                    </button>
                  )
                })}
                <button
                  onClick={() => selectEnvForEdit(null)}
                  className="flex items-center gap-1.5 rounded-md px-3 py-1 text-[11.5px] font-medium transition-colors hover:border-[var(--text-3)]"
                  style={{ background: 'transparent', border: '1px dashed var(--border)', color: 'var(--text-3)' }}
                >
                  <Plus className="h-3 w-3" />
                  New
                </button>
              </div>
              {store.activeId && (
                <button
                  onClick={() => activateEnv(null)}
                  className="text-[11px] underline decoration-dotted hover:no-underline"
                  style={{ color: 'var(--text-dim)' }}
                >
                  Clear active env
                </button>
              )}
            </div>

            {/* Right: edit form */}
            {(selectedId || isNewEnv) && (
              <div>
                <p className="mb-2 text-[9.5px] uppercase tracking-[1px] font-semibold" style={{ color: 'var(--text-dim)' }}>
                  {isNewEnv ? 'New Environment' : 'Edit Environment'}
                </p>
                <div className="space-y-2">
                  <input
                    type="text"
                    placeholder="Name (e.g. Staging)"
                    value={form.name}
                    onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))}
                    className="w-full rounded-md border px-3 py-[6px] text-[12px] outline-none transition-colors"
                    style={INPUT_STYLE}
                    onFocus={focusInput}
                    onBlur={blurInput}
                  />
                  <input
                    type="url"
                    placeholder="Base URL (e.g. https://staging.api.example.com)"
                    value={form.baseUrl}
                    onChange={(e) => setForm((f) => ({ ...f, baseUrl: e.target.value }))}
                    className="w-full rounded-md border px-3 py-[6px] text-[12px] outline-none transition-colors"
                    style={INPUT_STYLE}
                    onFocus={focusInput}
                    onBlur={blurInput}
                  />
                  <input
                    type="password"
                    placeholder="Bearer token (optional)"
                    value={form.bearerToken}
                    onChange={(e) => setForm((f) => ({ ...f, bearerToken: e.target.value }))}
                    className="w-full rounded-md border px-3 py-[6px] text-[12px] outline-none transition-colors"
                    style={INPUT_STYLE}
                    onFocus={focusInput}
                    onBlur={blurInput}
                  />
                  <div className="flex gap-2 pt-1">
                    <button
                      onClick={saveForm}
                      className="rounded-md px-4 py-[6px] text-[12px] font-semibold transition-all"
                      style={{ background: 'var(--cobalt)', color: 'var(--text-inverse)' }}
                      onMouseEnter={(e) => { e.currentTarget.style.background = 'var(--cobalt-mid)' }}
                      onMouseLeave={(e) => { e.currentTarget.style.background = 'var(--cobalt)' }}
                    >
                      {isNewEnv ? 'Add' : 'Save'}
                    </button>
                    {!isNewEnv && (
                      <button
                        onClick={deleteSelected}
                        className="flex items-center gap-1.5 rounded-md px-3 py-[6px] text-[12px] font-medium transition-colors"
                        style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)', color: 'var(--red)' }}
                      >
                        <Trash2 className="h-3 w-3" />
                        Delete
                      </button>
                    )}
                  </div>
                </div>
              </div>
            )}
          </div>
        )}
      </div>

      {/* Scalar iframe — fills remaining height */}
      <div className="flex-1 overflow-hidden">
        <iframe
          key={iframeKey}
          srcDoc={buildScalarHtml(activeUrl, theme, activeEnv)}
          title="API Playground"
          sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
          style={{ width: '100%', height: '100%', border: 'none' }}
        />
      </div>
    </div>
  )
}
