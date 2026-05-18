import { useState, useEffect } from 'react'
import { FlaskConical, Download, ChevronDown, ChevronUp, Clock } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import Badge from '../components/Badge'

const API = (import.meta.env.VITE_API_URL ?? 'http://localhost:8080') + '/v1'

interface Suite {
  id: string
  jira_key: string | null
  jira_summary: string | null
  collection_name: string
  test_count: number
  happy_count: number
  negative_count: number
  created_at: string
}

interface GenerateResult {
  id: string
  collection_name: string
  test_count: number
  happy_count: number
  negative_count: number
  collection_json: object
  created_at: string
}

export default function GenerateTestsPage() {
  const [suites, setSuites] = useState<Suite[]>([])
  const [loading, setLoading] = useState(true)

  // Form state
  const [jiraMode, setJiraMode] = useState<'key' | 'text'>('key')
  const [jiraKey, setJiraKey] = useState('')
  const [jiraText, setJiraText] = useState('')
  const [specYaml, setSpecYaml] = useState('')
  const [baseUrl, setBaseUrl] = useState('http://localhost:8080')
  const [generating, setGenerating] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [result, setResult] = useState<GenerateResult | null>(null)

  useEffect(() => {
    fetch(`${API}/generate-tests`, {
      headers: { Authorization: `Bearer ${localStorage.getItem('radarToken') ?? ''}` },
    })
      .then((r) => r.json())
      .then(setSuites)
      .catch(() => {})
      .finally(() => setLoading(false))
  }, [])

  async function handleGenerate(e: React.FormEvent) {
    e.preventDefault()
    setError(null)
    setResult(null)
    setGenerating(true)

    const body: Record<string, string | undefined> = {
      spec_yaml: specYaml,
      base_url: baseUrl,
      ...(jiraMode === 'key' ? { jira_key: jiraKey } : { jira_text: jiraText }),
    }

    try {
      const resp = await fetch(`${API}/generate-tests`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Authorization: `Bearer ${localStorage.getItem('radarToken') ?? ''}`,
        },
        body: JSON.stringify(body),
      })

      if (!resp.ok) {
        const err = await resp.json().catch(() => ({ error: resp.statusText }))
        throw new Error(err.error ?? resp.statusText)
      }

      const data: GenerateResult = await resp.json()
      setResult(data)
      setSuites((prev) => [
        {
          id: data.id,
          jira_key: jiraMode === 'key' ? jiraKey : null,
          jira_summary: null,
          collection_name: data.collection_name,
          test_count: data.test_count,
          happy_count: data.happy_count,
          negative_count: data.negative_count,
          created_at: data.created_at,
        },
        ...prev,
      ])
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setGenerating(false)
    }
  }

  function downloadCollection(json: object, name: string) {
    const blob = new Blob([JSON.stringify(json, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `${name.replace(/[^a-z0-9]/gi, '_')}.postman_collection.json`
    a.click()
    URL.revokeObjectURL(url)
  }

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-8">
      <PageHeader
        icon={FlaskConical}
        title="Generate Postman Tests"
        subtitle="Generate happy-path and negative test cases from a Jira ticket and OpenAPI spec"
      />

      {/* ------------------------------------------------------------------ */}
      {/* Generation form                                                       */}
      {/* ------------------------------------------------------------------ */}
      <form
        onSubmit={handleGenerate}
        className="rounded-lg p-6 space-y-5"
        style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)' }}
      >
        {/* Jira input row */}
        <div className="space-y-2">
          <div className="flex items-center gap-3">
            <span className="text-[12px] font-semibold uppercase tracking-wide" style={{ color: 'var(--text-2)' }}>
              Jira ticket
            </span>
            <button
              type="button"
              onClick={() => setJiraMode(jiraMode === 'key' ? 'text' : 'key')}
              className="text-[11px] px-2 py-0.5 rounded"
              style={{ background: 'var(--bg-hover)', color: 'var(--cobalt-mid)' }}
            >
              {jiraMode === 'key' ? 'Switch to paste text' : 'Switch to ticket key'}
            </button>
          </div>

          {jiraMode === 'key' ? (
            <input
              type="text"
              placeholder="e.g. PROJ-123"
              value={jiraKey}
              onChange={(e) => setJiraKey(e.target.value)}
              className="w-full rounded-md px-3 py-2 text-[13px] outline-none"
              style={{
                background: 'var(--bg-base)',
                border: '1px solid var(--border)',
                color: 'var(--text-1)',
              }}
            />
          ) : (
            <textarea
              rows={5}
              placeholder="Paste the full Jira ticket text here (title on first line, description below)"
              value={jiraText}
              onChange={(e) => setJiraText(e.target.value)}
              className="w-full rounded-md px-3 py-2 text-[13px] outline-none resize-y"
              style={{
                background: 'var(--bg-base)',
                border: '1px solid var(--border)',
                color: 'var(--text-1)',
                fontFamily: 'var(--font-mono)',
              }}
            />
          )}
        </div>

        {/* Spec YAML */}
        <div className="space-y-2">
          <label className="text-[12px] font-semibold uppercase tracking-wide" style={{ color: 'var(--text-2)' }}>
            OpenAPI spec (YAML / JSON)
          </label>
          <textarea
            rows={10}
            placeholder="Paste your openapi.yaml or openapi.json content here…"
            value={specYaml}
            onChange={(e) => setSpecYaml(e.target.value)}
            required
            className="w-full rounded-md px-3 py-2 text-[12px] outline-none resize-y"
            style={{
              background: 'var(--bg-base)',
              border: '1px solid var(--border)',
              color: 'var(--text-1)',
              fontFamily: 'var(--font-mono)',
            }}
          />
        </div>

        {/* Base URL */}
        <div className="space-y-2">
          <label className="text-[12px] font-semibold uppercase tracking-wide" style={{ color: 'var(--text-2)' }}>
            API base URL
          </label>
          <input
            type="text"
            value={baseUrl}
            onChange={(e) => setBaseUrl(e.target.value)}
            className="w-full rounded-md px-3 py-2 text-[13px] outline-none"
            style={{
              background: 'var(--bg-base)',
              border: '1px solid var(--border)',
              color: 'var(--text-1)',
            }}
          />
        </div>

        {error && (
          <p className="text-[12px] px-3 py-2 rounded" style={{ background: 'var(--red-bg, #2a0a0a)', color: 'var(--red, #f87171)' }}>
            {error}
          </p>
        )}

        <button
          type="submit"
          disabled={generating || !specYaml.trim() || (jiraMode === 'key' ? !jiraKey.trim() : !jiraText.trim())}
          className="flex items-center gap-2 px-4 py-2 rounded-md text-[13px] font-medium transition-opacity disabled:opacity-40"
          style={{ background: 'var(--cobalt)', color: 'var(--text-inverse)' }}
        >
          <FlaskConical className="h-4 w-4" />
          {generating ? 'Generating…' : 'Generate Tests'}
        </button>
      </form>

      {/* ------------------------------------------------------------------ */}
      {/* Result card                                                           */}
      {/* ------------------------------------------------------------------ */}
      {result && (
        <div
          className="rounded-lg p-6 space-y-4"
          style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)' }}
        >
          <div className="flex items-start justify-between gap-4">
            <div>
              <p className="font-semibold text-[14px]" style={{ color: 'var(--text-1)' }}>
                {result.collection_name}
              </p>
              <div className="flex gap-2 mt-1.5">
                <Badge variant="success">{result.happy_count} happy-path</Badge>
                <Badge variant="danger">{result.negative_count} negative</Badge>
                <Badge>{result.test_count} total</Badge>
              </div>
            </div>
            <button
              onClick={() => downloadCollection(result.collection_json, result.collection_name)}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium shrink-0"
              style={{ background: 'var(--bg-hover)', color: 'var(--text-1)', border: '1px solid var(--border)' }}
            >
              <Download className="h-3.5 w-3.5" />
              Download Collection
            </button>
          </div>
          <CollapsibleJson label="Preview collection JSON" json={result.collection_json} />
        </div>
      )}

      {/* ------------------------------------------------------------------ */}
      {/* History table                                                         */}
      {/* ------------------------------------------------------------------ */}
      <section className="space-y-3">
        <h2 className="text-[13px] font-semibold uppercase tracking-wider" style={{ color: 'var(--text-2)' }}>
          Previous generations
        </h2>

        {loading ? (
          <p className="text-[13px]" style={{ color: 'var(--text-3)' }}>Loading…</p>
        ) : suites.length === 0 ? (
          <p className="text-[13px]" style={{ color: 'var(--text-3)' }}>No test suites generated yet.</p>
        ) : (
          <div className="rounded-lg overflow-hidden" style={{ border: '1px solid var(--border)' }}>
            <table className="w-full text-[12.5px]">
              <thead>
                <tr style={{ background: 'var(--bg-surface)', borderBottom: '1px solid var(--border)' }}>
                  {['Collection', 'Jira key', 'Tests', 'Generated'].map((h) => (
                    <th key={h} className="px-4 py-2.5 text-left font-semibold uppercase tracking-wide text-[10.5px]"
                      style={{ color: 'var(--text-dim)' }}>
                      {h}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {suites.map((s) => (
                  <tr key={s.id} style={{ borderBottom: '1px solid var(--border)' }}>
                    <td className="px-4 py-3" style={{ color: 'var(--text-1)' }}>{s.collection_name}</td>
                    <td className="px-4 py-3" style={{ color: 'var(--text-2)' }}>{s.jira_key ?? '—'}</td>
                    <td className="px-4 py-3">
                      <div className="flex gap-1.5">
                        <Badge variant="success">{s.happy_count}✓</Badge>
                        <Badge variant="danger">{s.negative_count}✗</Badge>
                      </div>
                    </td>
                    <td className="px-4 py-3 flex items-center gap-1" style={{ color: 'var(--text-3)' }}>
                      <Clock className="h-3 w-3" />
                      {new Date(s.created_at).toLocaleString()}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </div>
  )
}

function CollapsibleJson({ label, json }: { label: string; json: object }) {
  const [open, setOpen] = useState(false)
  return (
    <div className="rounded-md overflow-hidden" style={{ border: '1px solid var(--border)' }}>
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="w-full flex items-center justify-between px-3 py-2 text-[12px]"
        style={{ background: 'var(--bg-hover)', color: 'var(--text-2)' }}
      >
        {label}
        {open ? <ChevronUp className="h-3.5 w-3.5" /> : <ChevronDown className="h-3.5 w-3.5" />}
      </button>
      {open && (
        <pre
          className="p-3 text-[11px] overflow-auto max-h-96"
          style={{ fontFamily: 'var(--font-mono)', color: 'var(--text-1)', background: 'var(--bg-base)' }}
        >
          {JSON.stringify(json, null, 2)}
        </pre>
      )}
    </div>
  )
}
