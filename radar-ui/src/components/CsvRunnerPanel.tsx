import { useRef, useState, useCallback, useEffect } from 'react'
import {
  Upload, Play, Square, Download, FileDown,
  CheckCircle, XCircle, AlertCircle, Loader2, Eye, EyeOff,
} from 'lucide-react'
import { parseCsv } from '../lib/csvParser'
import { extractVariables, type PlaygroundRequest } from '../lib/variableExtractor'
import { resolveRequest, maskSecrets } from '../lib/variableResolver'
import { exportResults, exportFailedRows, triggerDownload, type RowResult } from '../lib/csvExporter'

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface MappingEntry {
  variable: string
  status: 'mapped' | 'unmapped'
  column: string | null
}

interface CsvRunJob {
  id: string
  name: string
  status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled'
  total_rows: number
  completed_rows: number
  error_count: number
  error_message: string | null
  created_at: string
  started_at: string | null
  completed_at: string | null
}

// ---------------------------------------------------------------------------
// Request template builder
// ---------------------------------------------------------------------------

const METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS']

function RequestBuilder({ request, onChange }: {
  request: PlaygroundRequest
  onChange: (r: PlaygroundRequest) => void
}) {
  function setHeader(i: number, field: 'key' | 'value', val: string) {
    const headers = [...request.headers]
    headers[i] = { ...headers[i], [field]: val }
    onChange({ ...request, headers })
  }

  function addHeader() {
    onChange({ ...request, headers: [...request.headers, { key: '', value: '' }] })
  }

  function removeHeader(i: number) {
    onChange({ ...request, headers: request.headers.filter((_, idx) => idx !== i) })
  }

  const inputCls = 'w-full rounded border px-2.5 py-1.5 text-[12.5px] outline-none focus:ring-1 font-mono'
  const style = { background: 'var(--bg-raised)', border: '1px solid var(--border)', color: 'var(--text-1)' }

  return (
    <div className="space-y-3">
      <div className="flex gap-2">
        <select
          value={request.method}
          onChange={e => onChange({ ...request, method: e.target.value })}
          className="rounded border px-2 py-1.5 text-[12px] font-semibold outline-none"
          style={{ ...style, width: '90px', fontFamily: 'var(--font-mono)' }}
        >
          {METHODS.map(m => <option key={m}>{m}</option>)}
        </select>
        <input
          type="text"
          value={request.url}
          onChange={e => onChange({ ...request, url: e.target.value })}
          placeholder="https://api.example.com/users/{{user_id}}"
          className={`${inputCls} flex-1`}
          style={style}
        />
      </div>

      <div>
        <div className="flex items-center justify-between mb-1.5">
          <p className="text-[11px] font-semibold uppercase tracking-[0.6px]" style={{ color: 'var(--text-dim)' }}>Headers</p>
          <button onClick={addHeader} className="text-[11px]" style={{ color: 'var(--cobalt-mid)' }}>+ Add</button>
        </div>
        {request.headers.map((h, i) => (
          <div key={i} className="flex gap-2 mb-1">
            <input value={h.key} onChange={e => setHeader(i, 'key', e.target.value)} placeholder="Header-Name" className={`${inputCls} flex-1`} style={style} />
            <input value={h.value} onChange={e => setHeader(i, 'value', e.target.value)} placeholder="value or {{var}}" className={`${inputCls} flex-1`} style={style} />
            <button onClick={() => removeHeader(i)} style={{ color: 'var(--text-dim)' }}>
              <XCircle className="h-3.5 w-3.5" />
            </button>
          </div>
        ))}
      </div>

      {['POST', 'PUT', 'PATCH'].includes(request.method) && (
        <div>
          <p className="text-[11px] font-semibold uppercase tracking-[0.6px] mb-1.5" style={{ color: 'var(--text-dim)' }}>Body</p>
          <textarea
            value={request.body}
            onChange={e => onChange({ ...request, body: e.target.value })}
            placeholder={'{"id": "{{user_id}}"}'}
            rows={4}
            className={inputCls}
            style={{ ...style, resize: 'vertical' }}
          />
        </div>
      )}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Main CsvRunnerPanel
// ---------------------------------------------------------------------------

const DEFAULT_REQUEST: PlaygroundRequest = {
  url: '',
  method: 'GET',
  headers: [],
  body: '',
}

export default function CsvRunnerPanel() {
  const fileRef = useRef<HTMLInputElement>(null)

  const [request, setRequest] = useState<PlaygroundRequest>(DEFAULT_REQUEST)
  const [parseError, setParseError] = useState<string | null>(null)
  const [headers, setHeaders] = useState<string[]>([])
  const [rows, setRows] = useState<Record<string, string>[]>([])
  const [mapping, setMapping] = useState<MappingEntry[]>([])
  const [showPreview, setShowPreview] = useState(false)

  // Server-side job state
  const [jobId, setJobId] = useState<string | null>(null)
  const [job, setJob] = useState<CsvRunJob | null>(null)
  const [results, setResults] = useState<RowResult[]>([])
  const [runError, setRunError] = useState<string | null>(null)
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null)

  // O-5: response body capture
  const [captureBody, setCaptureBody] = useState(false)
  const [responseBodies, setResponseBodies] = useState<Record<number, string>>({})
  const [expandedBodies, setExpandedBodies] = useState<Set<number>>(new Set())

  const running = job !== null && (job.status === 'pending' || job.status === 'running')

  // Recompute mapping whenever request or CSV headers change
  useEffect(() => {
    const variables = extractVariables(request)
    const newMapping: MappingEntry[] = variables.map(v => ({
      variable: v,
      status: headers.includes(v) ? 'mapped' : 'unmapped',
      column: headers.includes(v) ? v : null,
    }))
    setMapping(newMapping)
  }, [request, headers])

  // Cleanup polling on unmount
  useEffect(() => {
    return () => {
      if (pollRef.current) clearInterval(pollRef.current)
    }
  }, [])

  // Warn before tab close during run
  useEffect(() => {
    if (!running) return
    const handler = (e: BeforeUnloadEvent) => {
      e.preventDefault()
      e.returnValue = 'A run is in progress — leaving will cancel it'
    }
    window.addEventListener('beforeunload', handler)
    return () => window.removeEventListener('beforeunload', handler)
  }, [running])

  function onFileChange(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0]
    if (!file) return
    const reader = new FileReader()
    reader.onload = ev => {
      const text = ev.target?.result as string
      const result = parseCsv(text)
      if (result.error) {
        setParseError(result.error)
        setHeaders([])
        setRows([])
      } else {
        setParseError(null)
        setHeaders(result.headers)
        setRows(result.rows)
      }
    }
    reader.readAsText(file, 'utf-8')
  }

  async function loadResults(id: string) {
    try {
      const resp = await fetch(`/v1/csv-runs/${id}/results?limit=500`)
      if (!resp.ok) return
      const data = await resp.json() as Array<{
        row_number: number
        http_status: number | null
        duration_ms: number
        error: string | null
        url: string
        response_body: string | null
      }>
      // Map server result shape to RowResult shape used by csvExporter.
      const mapped: RowResult[] = data.map((r, idx) => ({
        rowNumber: r.row_number,
        httpStatus: r.http_status,
        durationMs: r.duration_ms,
        error: r.error,
        url: r.url,
        originalRow: rows[idx] ?? {},
      }))
      setResults(mapped)
      // O-5: collect captured response bodies indexed by row number
      const bodies: Record<number, string> = {}
      data.forEach(r => { if (r.response_body) bodies[r.row_number] = r.response_body })
      setResponseBodies(bodies)
    } catch {
      // Non-fatal: results already partially shown via job status
    }
  }

  const pollJob = useCallback(async (id: string) => {
    try {
      const resp = await fetch(`/v1/csv-runs/${id}`)
      if (!resp.ok) return
      const data = await resp.json() as CsvRunJob
      setJob(data)
      if (data.status === 'completed' || data.status === 'failed' || data.status === 'cancelled') {
        if (pollRef.current) clearInterval(pollRef.current)
        pollRef.current = null
        await loadResults(id)
      }
    } catch {
      // Keep polling; transient errors are expected
    }
  }, [rows])

  const runBatch = useCallback(async () => {
    if (rows.length === 0 || !request.url) return
    setRunError(null)
    setResults([])
    setJob(null)
    setJobId(null)

    try {
      setResponseBodies({})
      setExpandedBodies(new Set())
      const resp = await fetch('/v1/csv-runs', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          name: `CSV run — ${new Date().toLocaleTimeString()}`,
          request: { ...request, capture_body: captureBody },
          rows,
        }),
      })
      if (!resp.ok) {
        const body = await resp.json().catch(() => ({})) as { error?: string }
        throw new Error(body.error ?? `HTTP ${resp.status}`)
      }
      const data = await resp.json() as { id: string; status: string; total_rows: number }
      setJobId(data.id)
      setJob({
        id: data.id,
        name: '',
        status: 'pending',
        total_rows: data.total_rows,
        completed_rows: 0,
        error_count: 0,
        error_message: null,
        created_at: new Date().toISOString(),
        started_at: null,
        completed_at: null,
      })

      // Start polling every 2s
      if (pollRef.current) clearInterval(pollRef.current)
      pollRef.current = setInterval(() => pollJob(data.id), 2000)
    } catch (err) {
      setRunError((err as Error).message)
    }
  }, [rows, request, pollJob])

  async function cancel() {
    if (!jobId) return
    try {
      await fetch(`/v1/csv-runs/${jobId}`, { method: 'DELETE' })
    } catch {
      // Best-effort
    }
    if (pollRef.current) clearInterval(pollRef.current)
    pollRef.current = null
    if (jobId) await pollJob(jobId) // fetch final status
  }

  const unresolvedVars = mapping.filter(m => m.status === 'unmapped')
  const successCount = results.filter(r => r.httpStatus != null && r.httpStatus < 400).length
  const failedCount = results.filter(r => r.error != null || (r.httpStatus != null && r.httpStatus >= 400)).length
  const avgDuration = results.length > 0
    ? Math.round(results.reduce((s, r) => s + r.durationMs, 0) / results.length)
    : 0

  const progress = job?.completed_rows ?? 0
  const totalRows = job?.total_rows ?? rows.length

  const statusColor = (r: RowResult) => {
    if (r.error) return 'var(--red)'
    if (r.httpStatus != null && r.httpStatus < 400) return 'var(--green)'
    return 'var(--red)'
  }

  return (
    <div className="px-14 py-8 max-w-5xl space-y-6">

      {/* Request template */}
      <div className="rounded-lg border px-6 py-5" style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}>
        <p className="text-[12.5px] font-semibold mb-4" style={{ color: 'var(--text-1)' }}>Request Template</p>
        <p className="text-[11.5px] mb-4" style={{ color: 'var(--text-3)' }}>
          Use <code className="font-mono" style={{ color: 'var(--cobalt-mid)' }}>{'{{column_name}}'}</code> placeholders — one value per CSV row.
          Requests are executed server-side — target URLs must be reachable from the radar-api host.
        </p>
        <RequestBuilder request={request} onChange={setRequest} />
      </div>

      {/* CSV Upload */}
      <div className="rounded-lg border px-6 py-5" style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}>
        <div className="flex items-center justify-between mb-4">
          <p className="text-[12.5px] font-semibold" style={{ color: 'var(--text-1)' }}>CSV Data</p>
          <button
            onClick={() => fileRef.current?.click()}
            className="flex items-center gap-1.5 rounded-md px-3 py-1.5 text-[12px] font-medium transition-opacity hover:opacity-80"
            style={{ background: 'var(--bg-raised)', color: 'var(--text-2)', border: '1px solid var(--border)' }}
          >
            <Upload className="h-3.5 w-3.5" />
            Upload CSV
          </button>
          <input ref={fileRef} type="file" accept=".csv,text/csv" onChange={onFileChange} className="hidden" />
        </div>

        {parseError && (
          <div className="flex items-center gap-2 text-[12.5px] mb-3" style={{ color: 'var(--red)' }}>
            <AlertCircle className="h-4 w-4 flex-shrink-0" />
            {parseError}
          </div>
        )}

        {headers.length > 0 && (
          <div className="space-y-3">
            <p className="text-[11.5px]" style={{ color: 'var(--text-dim)' }}>
              {rows.length} rows loaded &mdash; {headers.length} columns
            </p>

            {mapping.length > 0 && (
              <div>
                <p className="text-[10.5px] font-semibold uppercase tracking-[0.7px] mb-2" style={{ color: 'var(--text-dim)' }}>Variable mapping</p>
                <div className="space-y-1">
                  {mapping.map(m => (
                    <div key={m.variable} className="flex items-center gap-3">
                      <code className="text-[12px] font-mono w-40" style={{ color: 'var(--cobalt-mid)' }}>{`{{${m.variable}}}`}</code>
                      {m.status === 'mapped'
                        ? <CheckCircle className="h-3.5 w-3.5" style={{ color: 'var(--green)' }} />
                        : <XCircle className="h-3.5 w-3.5" style={{ color: 'var(--red)' }} />
                      }
                      <span className="text-[11.5px]" style={{ color: m.status === 'mapped' ? 'var(--text-2)' : 'var(--red)' }}>
                        {m.status === 'mapped' ? `← column "${m.column}"` : 'column not found in CSV'}
                      </span>
                    </div>
                  ))}
                </div>
                {/* Unused columns */}
                {headers.filter(h => !mapping.find(m => m.column === h)).length > 0 && (
                  <p className="text-[11.5px] mt-2" style={{ color: 'var(--text-dim)' }}>
                    Unused columns: {headers.filter(h => !mapping.find(m => m.column === h)).join(', ')}
                  </p>
                )}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Preview table */}
      {rows.length > 0 && (
        <div className="rounded-lg border overflow-hidden" style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}>
          <div className="flex items-center justify-between px-5 py-3 border-b" style={{ borderColor: 'var(--border)' }}>
            <p className="text-[12.5px] font-semibold" style={{ color: 'var(--text-1)' }}>Preview (first {Math.min(10, rows.length)} rows)</p>
            <button
              onClick={() => setShowPreview(p => !p)}
              className="flex items-center gap-1.5 text-[12px]"
              style={{ color: 'var(--cobalt-mid)' }}
            >
              {showPreview ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
              {showPreview ? 'Hide' : 'Show'} preview
            </button>
          </div>
          {showPreview && (
            <div className="overflow-x-auto">
              {unresolvedVars.length > 0 && (
                <div className="px-5 py-2 flex items-center gap-2 border-b" style={{ borderColor: 'var(--border)', background: 'color-mix(in srgb, var(--red) 8%, transparent)' }}>
                  <AlertCircle className="h-3.5 w-3.5 flex-shrink-0" style={{ color: 'var(--red)' }} />
                  <p className="text-[12px]" style={{ color: 'var(--red)' }}>
                    {unresolvedVars.length} variable(s) unresolved: {unresolvedVars.map(m => `{{${m.variable}}}`).join(', ')}
                  </p>
                </div>
              )}
              <table className="w-full text-[11.5px]">
                <thead>
                  <tr style={{ borderBottom: '1px solid var(--border)' }}>
                    <th className="px-4 py-2 text-left font-medium" style={{ color: 'var(--text-dim)' }}>#</th>
                    <th className="px-4 py-2 text-left font-medium" style={{ color: 'var(--text-dim)' }}>URL</th>
                    <th className="px-4 py-2 text-left font-medium" style={{ color: 'var(--text-dim)' }}>Body preview</th>
                    <th className="px-4 py-2 text-left font-medium" style={{ color: 'var(--text-dim)' }}>Headers</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.slice(0, 10).map((row, i) => {
                    const resolved = resolveRequest(request, row)
                    const maskedHeaders = maskSecrets(resolved.headers)
                    const hasUnresolved = resolved.unresolved.length > 0
                    return (
                      <tr key={i} style={{ borderBottom: '1px solid var(--border)' }}>
                        <td className="px-4 py-2" style={{ color: 'var(--text-dim)' }}>{i + 1}</td>
                        <td className="px-4 py-2 font-mono max-w-[280px] truncate" style={{ color: hasUnresolved ? 'var(--red)' : 'var(--text-2)' }}>
                          {resolved.url || <span style={{ color: 'var(--text-dim)' }}>—</span>}
                        </td>
                        <td className="px-4 py-2 font-mono max-w-[200px] truncate" style={{ color: 'var(--text-3)' }}>
                          {resolved.body ? resolved.body.slice(0, 200) : '—'}
                        </td>
                        <td className="px-4 py-2 max-w-[200px] truncate" style={{ color: 'var(--text-dim)' }}>
                          {Object.entries(maskedHeaders).map(([k, v]) => `${k}: ${v}`).join(', ') || '—'}
                        </td>
                      </tr>
                    )
                  })}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}

      {/* Run controls */}
      {rows.length > 0 && request.url && (
        <div className="space-y-3">
          {/* Capture body option (O-5) */}
          <label className="flex items-center gap-2 text-[12px] cursor-pointer select-none" style={{ color: 'var(--text-2)' }}>
            <input
              type="checkbox"
              checked={captureBody}
              onChange={e => setCaptureBody(e.target.checked)}
              className="rounded"
            />
            Capture response body (first 10 KB per row)
          </label>
        <div className="flex items-center gap-3">
          {!running ? (
            <button
              onClick={runBatch}
              disabled={running || unresolvedVars.length > 0}
              title={unresolvedVars.length > 0 ? `Resolve all variables before running: ${unresolvedVars.map(m => `{{${m.variable}}}`).join(', ')}` : undefined}
              className="flex items-center gap-2 rounded-md px-4 py-2 text-[12.5px] font-semibold disabled:opacity-40 disabled:cursor-not-allowed"
              style={{ background: 'var(--cobalt)', color: 'var(--text-inverse)' }}
            >
              <Play className="h-3.5 w-3.5" />
              Run Batch ({rows.length} rows)
            </button>
          ) : (
            <button
              onClick={cancel}
              className="flex items-center gap-2 rounded-md px-4 py-2 text-[12.5px] font-semibold"
              style={{ background: 'var(--red)', color: '#fff' }}
            >
              <Square className="h-3.5 w-3.5" />
              Cancel
            </button>
          )}
          {running && (
            <div className="flex items-center gap-2 text-[12px]" style={{ color: 'var(--text-3)' }}>
              <Loader2 className="h-4 w-4 animate-spin" />
              {progress}/{totalRows} complete
              {job?.status === 'pending' && <span style={{ color: 'var(--text-dim)' }}>(queued…)</span>}
            </div>
          )}
        </div>
        </div>
      )}

      {/* Run-level error */}
      {runError && (
        <div
          className="flex items-start gap-2 rounded-md px-3 py-2.5 text-[12px]"
          style={{ background: 'var(--red-bg)', border: '1px solid var(--red-dim)', color: 'var(--red)' }}
        >
          <AlertCircle className="h-4 w-4 mt-0.5 flex-shrink-0" />
          {runError}
        </div>
      )}

      {/* Results */}
      {(results.length > 0 || (job && job.status !== 'pending' && job.status !== 'running')) && (
        <div className="rounded-lg border overflow-hidden" style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}>
          <div className="flex items-center justify-between px-5 py-3 border-b" style={{ borderColor: 'var(--border)' }}>
            <div className="flex items-center gap-6">
              <p className="text-[12.5px] font-semibold" style={{ color: 'var(--text-1)' }}>Results</p>
              {results.length > 0 && (
                <>
                  <span className="text-[12px]" style={{ color: 'var(--green)' }}>{successCount} ok</span>
                  {failedCount > 0 && <span className="text-[12px]" style={{ color: 'var(--red)' }}>{failedCount} failed</span>}
                  <span className="text-[12px]" style={{ color: 'var(--text-dim)' }}>{avgDuration}ms avg</span>
                </>
              )}
              {job && (
                <span
                  className="text-[11px] rounded px-1.5 py-0.5 font-medium"
                  style={{
                    background: job.status === 'completed' ? 'var(--green-bg, rgba(0,200,100,0.1))' :
                               job.status === 'failed' ? 'var(--red-bg)' :
                               job.status === 'cancelled' ? 'var(--bg-raised)' : 'var(--bg-raised)',
                    color: job.status === 'completed' ? 'var(--green)' :
                           job.status === 'failed' ? 'var(--red)' :
                           'var(--text-dim)',
                  }}
                >
                  {job.status}
                </span>
              )}
            </div>
            {results.length > 0 && (
              <div className="flex items-center gap-2">
                <button
                  onClick={() => triggerDownload(exportResults(results), 'run-results.csv')}
                  className="flex items-center gap-1.5 rounded px-2.5 py-1.5 text-[11.5px] font-medium"
                  style={{ background: 'var(--bg-raised)', color: 'var(--text-2)', border: '1px solid var(--border)' }}
                >
                  <Download className="h-3.5 w-3.5" />
                  Export Results
                </button>
                {failedCount > 0 && (
                  <button
                    onClick={() => triggerDownload(exportFailedRows(results), 'failed-rows.csv')}
                    className="flex items-center gap-1.5 rounded px-2.5 py-1.5 text-[11.5px] font-medium"
                    style={{ background: 'var(--bg-raised)', color: 'var(--red)', border: '1px solid var(--border)' }}
                  >
                    <FileDown className="h-3.5 w-3.5" />
                    Failed Rows
                  </button>
                )}
              </div>
            )}
          </div>

          {/* Progress bar while running */}
          {running && totalRows > 0 && (
            <div className="w-full h-1" style={{ background: 'var(--bg-raised)' }}>
              <div
                className="h-1 transition-all"
                style={{ width: `${(progress / totalRows) * 100}%`, background: 'var(--cobalt)' }}
              />
            </div>
          )}

          {results.length > 0 && (
            <table className="w-full text-[11.5px]">
              <thead>
                <tr style={{ borderBottom: '1px solid var(--border)' }}>
                  <th className="px-4 py-2 text-left font-medium" style={{ color: 'var(--text-dim)' }}>#</th>
                  <th className="px-4 py-2 text-left font-medium" style={{ color: 'var(--text-dim)' }}>Status</th>
                  <th className="px-4 py-2 text-left font-medium" style={{ color: 'var(--text-dim)' }}>HTTP</th>
                  <th className="px-4 py-2 text-left font-medium" style={{ color: 'var(--text-dim)' }}>Duration</th>
                  <th className="px-4 py-2 text-left font-medium" style={{ color: 'var(--text-dim)' }}>URL</th>
                  <th className="px-4 py-2 text-left font-medium" style={{ color: 'var(--text-dim)' }}>Error / Body</th>
                </tr>
              </thead>
              <tbody>
                {results.map(r => {
                  const body = responseBodies[r.rowNumber]
                  const isExpanded = expandedBodies.has(r.rowNumber)
                  return (
                    <>
                      <tr key={r.rowNumber} style={{ borderBottom: body && isExpanded ? 'none' : '1px solid var(--border)' }}>
                        <td className="px-4 py-2" style={{ color: 'var(--text-dim)' }}>{r.rowNumber}</td>
                        <td className="px-4 py-2">
                          {r.error
                            ? <XCircle className="h-3.5 w-3.5" style={{ color: 'var(--red)' }} />
                            : r.httpStatus != null && r.httpStatus < 400
                              ? <CheckCircle className="h-3.5 w-3.5" style={{ color: 'var(--green)' }} />
                              : <XCircle className="h-3.5 w-3.5" style={{ color: 'var(--red)' }} />
                          }
                        </td>
                        <td className="px-4 py-2 font-mono" style={{ color: statusColor(r) }}>
                          {r.httpStatus ?? '—'}
                        </td>
                        <td className="px-4 py-2" style={{ color: 'var(--text-3)' }}>{r.durationMs}ms</td>
                        <td className="px-4 py-2 font-mono truncate max-w-[260px]" style={{ color: 'var(--text-2)' }}>{r.url}</td>
                        <td className="px-4 py-2 max-w-[200px]">
                          {r.error
                            ? <span className="truncate" style={{ color: 'var(--red)' }}>{r.error}</span>
                            : body
                              ? (
                                <button
                                  onClick={() => setExpandedBodies(prev => {
                                    const next = new Set(prev)
                                    if (next.has(r.rowNumber)) next.delete(r.rowNumber)
                                    else next.add(r.rowNumber)
                                    return next
                                  })}
                                  className="text-[11px] underline"
                                  style={{ color: 'var(--cobalt-mid)' }}
                                >
                                  {isExpanded ? 'hide body' : 'show body'}
                                </button>
                              )
                              : <span style={{ color: 'var(--text-dim)' }}>—</span>
                          }
                        </td>
                      </tr>
                      {body && isExpanded && (
                        <tr key={`body-${r.rowNumber}`} style={{ borderBottom: '1px solid var(--border)' }}>
                          <td colSpan={6} className="px-4 pb-3 pt-0">
                            <pre
                              className="rounded p-3 text-[11px] font-mono overflow-x-auto max-h-40"
                              style={{ background: 'var(--bg-raised)', color: 'var(--text-2)', whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}
                            >
                              {body}
                            </pre>
                          </td>
                        </tr>
                      )}
                    </>
                  )
                })}
              </tbody>
            </table>
          )}

          {results.length === 0 && job && (job.status === 'failed' || job.status === 'cancelled') && (
            <div className="px-5 py-4 text-[12px]" style={{ color: 'var(--text-dim)' }}>
              {job.error_message ?? 'No results recorded.'}
            </div>
          )}
        </div>
      )}
    </div>
  )
}
