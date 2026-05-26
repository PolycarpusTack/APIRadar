import { useRef, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Rows, Upload, X, CheckCircle, XCircle, Loader, Play, AlertCircle } from 'lucide-react'
import Badge from './Badge'
import { parseCsv } from '../lib/csvParser'

interface BatchItem {
  label: string
  service_id: string
  format: string
  base_url: string
  head_url: string
}

interface BatchResult {
  label: string
  status: 'done' | 'error'
  diff_id?: string
  breaking_count: number
  changes_count: number
  error?: string
}

const EXAMPLE_CSV = `label,format,base_url,head_url
"Payment API v1→v2",openapi,https://example.com/api/v1/openapi.yaml,https://example.com/api/v2/openapi.yaml
"User Service",,https://example.com/users/v1.json,https://example.com/users/v2.json`

function toBatchItems(csvText: string): { items: BatchItem[]; error: string | null } {
  const parsed = parseCsv(csvText)
  if (parsed.error) return { items: [], error: parsed.error }

  const items: BatchItem[] = parsed.rows
    .map(row => ({
      label: row['label'] ?? '',
      service_id: row['service_id'] ?? '',
      format: row['format'] || 'openapi',
      base_url: row['base_url'] || row['base'] || '',
      head_url: row['head_url'] || row['head'] || '',
    }))
    .filter(r => r.base_url && r.head_url)

  return { items, error: null }
}

export default function BatchComparePanel({ onClose }: { onClose?: () => void }) {
  const navigate = useNavigate()
  const fileRef = useRef<HTMLInputElement>(null)
  const [csvText, setCsvText] = useState('')
  const [running, setRunning] = useState(false)
  const [results, setResults] = useState<BatchResult[] | null>(null)
  const [error, setError] = useState<string | null>(null)

  function handleFile(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0]
    if (!file) return
    const reader = new FileReader()
    reader.onload = () => setCsvText(reader.result as string)
    reader.readAsText(file)
    e.target.value = ''
  }

  async function handleRun() {
    setError(null)
    setResults(null)

    const { items, error: parseError } = toBatchItems(csvText)
    if (parseError) {
      setError(parseError)
      return
    }
    if (items.length === 0) {
      setError('No valid rows found. Ensure your CSV has base_url and head_url columns.')
      return
    }
    if (items.length > 50) {
      setError('Maximum 50 rows per batch.')
      return
    }

    setRunning(true)
    try {
      const resp = await fetch('/v1/compare/batch', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(items),
      })
      if (!resp.ok) {
        const body = await resp.json().catch(() => ({})) as { error?: string }
        throw new Error(body.error ?? `HTTP ${resp.status}`)
      }
      const data = await resp.json() as BatchResult[]
      setResults(data)
    } catch (err) {
      setError((err as Error).message)
    } finally {
      setRunning(false)
    }
  }

  const hasContent = csvText.trim().length > 0
  const { items: previewItems } = hasContent ? toBatchItems(csvText) : { items: [] }

  return (
    <div
      className="rounded-lg border p-6"
      style={{ background: 'var(--bg-surface)', border: '1px solid var(--border)' }}
    >
      <div className="flex items-center justify-between mb-5">
        <div className="flex items-center gap-2">
          <Rows className="h-4 w-4" style={{ color: 'var(--cobalt-mid)' }} />
          <p className="text-[13px] font-semibold" style={{ color: 'var(--text-1)' }}>
            Batch Compare via CSV
          </p>
          <span
            className="rounded px-1.5 py-0.5 text-[10px] font-medium"
            style={{ background: 'rgba(56,5,227,0.12)', color: 'var(--cobalt-mid)' }}
          >
            max 50 rows
          </span>
        </div>
        {onClose && (
          <button onClick={onClose} style={{ color: 'var(--text-3)' }}>
            <X className="h-4 w-4" />
          </button>
        )}
      </div>

      <div className="flex flex-col gap-4">
        {/* Format hint */}
        <div
          className="rounded-md p-3"
          style={{ background: 'var(--bg-raised)', border: '1px solid var(--border)' }}
        >
          <p
            className="mb-1"
            style={{
              fontFamily: 'var(--font-mono)',
              fontSize: '10px',
              fontWeight: 600,
              letterSpacing: '0.8px',
              textTransform: 'uppercase',
              color: 'var(--text-3)',
            }}
          >
            CSV columns
          </p>
          <p style={{ fontFamily: 'var(--font-mono)', fontSize: '11px', color: 'var(--text-2)' }}>
            <span style={{ color: 'var(--text-1)' }}>base_url</span>
            {', '}
            <span style={{ color: 'var(--text-1)' }}>head_url</span>
            {' '}
            <span style={{ color: 'var(--text-dim)' }}>(required)</span>
            {'  ·  label, service_id, format '}
            <span style={{ color: 'var(--text-dim)' }}>(optional)</span>
          </p>
          <p className="mt-1.5" style={{ fontFamily: 'var(--font-mono)', fontSize: '10.5px', color: 'var(--text-dim)' }}>
            Specs are fetched server-side — URLs must be reachable from the radar-api host.
          </p>
        </div>

        {/* Textarea + upload */}
        <div className="flex flex-col gap-1.5">
          <div className="flex items-center justify-between">
            <label
              className="text-[10.5px] font-semibold uppercase tracking-[0.8px]"
              style={{ color: 'var(--text-3)' }}
            >
              Paste or upload CSV
            </label>
            <button
              type="button"
              onClick={() => fileRef.current?.click()}
              className="flex items-center gap-1 text-[11px] rounded px-2 py-0.5 transition-colors hover:bg-[var(--bg-hover)]"
              style={{ color: 'var(--text-3)', border: '1px solid var(--border)' }}
            >
              <Upload className="h-3 w-3" />
              Load CSV
            </button>
          </div>
          <input
            ref={fileRef}
            type="file"
            accept=".csv,.txt"
            className="hidden"
            onChange={handleFile}
          />
          <textarea
            value={csvText}
            onChange={e => setCsvText(e.target.value)}
            rows={7}
            spellCheck={false}
            placeholder={EXAMPLE_CSV}
            className="w-full rounded-md p-3 text-[11.5px] leading-relaxed resize-y"
            style={{
              fontFamily: 'var(--font-mono)',
              background: 'var(--bg-input, var(--bg-raised))',
              border: '1px solid var(--border)',
              color: 'var(--text-1)',
              outline: 'none',
            }}
          />
          {hasContent && (
            <p className="text-[11px]" style={{ color: previewItems.length > 0 ? 'var(--text-3)' : 'var(--red)' }}>
              {previewItems.length > 0
                ? `${previewItems.length} row${previewItems.length !== 1 ? 's' : ''} parsed`
                : 'No valid rows — check that base_url and head_url columns are present'}
            </p>
          )}
        </div>

        {/* General error */}
        {error && (
          <div
            className="flex items-start gap-2 rounded-md px-3 py-2.5 text-[12px]"
            style={{ background: 'var(--red-bg)', border: '1px solid var(--red-dim)', color: 'var(--red)' }}
          >
            <AlertCircle className="h-4 w-4 mt-0.5 flex-shrink-0" />
            {error}
          </div>
        )}

        {/* Run button */}
        <div className="flex justify-end">
          <button
            onClick={handleRun}
            disabled={running || !hasContent || previewItems.length === 0}
            className="btn-primary flex items-center gap-2 rounded-md px-5 py-2 text-[12.5px] font-medium"
          >
            {running
              ? <Loader className="h-3.5 w-3.5 animate-spin" />
              : <Play className="h-3.5 w-3.5" />}
            {running ? `Running ${previewItems.length} comparison${previewItems.length !== 1 ? 's' : ''}…` : 'Run Batch'}
          </button>
        </div>

        {/* Results table */}
        {results && (
          <div className="flex flex-col gap-2">
            <p
              className="text-[10.5px] font-semibold uppercase tracking-[0.8px]"
              style={{ color: 'var(--text-3)' }}
            >
              Results — {results.filter(r => r.status === 'done').length}/{results.length} succeeded
              {results.some(r => r.breaking_count > 0) && (
                <span className="ml-2 font-normal normal-case" style={{ color: 'var(--red)' }}>
                  · Breaking changes detected
                </span>
              )}
            </p>
            <div
              className="overflow-x-auto rounded-md"
              style={{ border: '1px solid var(--border)' }}
            >
              <table className="w-full border-collapse">
                <thead>
                  <tr style={{ background: 'var(--bg-raised)' }}>
                    {['Label', 'Changes', 'Breaking', 'Status'].map(col => (
                      <th
                        key={col}
                        className="border-b px-3 py-2 text-left text-[10.5px] font-semibold uppercase tracking-[0.8px]"
                        style={{ borderColor: 'var(--border)', color: 'var(--text-3)' }}
                      >
                        {col}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {results.map((r, i) => (
                    <tr
                      key={i}
                      className={r.diff_id ? 'cursor-pointer transition-colors hover:bg-[var(--bg-hover)]' : ''}
                      style={{ borderBottom: '1px solid var(--border)' }}
                      onClick={() => r.diff_id && navigate(`/diffs/${r.diff_id}`)}
                    >
                      <td
                        className="px-3 py-2.5 font-medium"
                        style={{
                          color: 'var(--text-1)',
                          fontSize: '12.5px',
                          maxWidth: '320px',
                          overflow: 'hidden',
                          textOverflow: 'ellipsis',
                          whiteSpace: 'nowrap',
                        }}
                      >
                        {r.label || '—'}
                      </td>
                      <td className="px-3 py-2.5" style={{ fontSize: '12px', color: 'var(--text-2)' }}>
                        {r.status === 'done' ? r.changes_count : '—'}
                      </td>
                      <td className="px-3 py-2.5">
                        {r.status === 'done' ? (
                          r.breaking_count > 0
                            ? <Badge variant="err">{r.breaking_count}</Badge>
                            : <span style={{ color: 'var(--text-dim)', fontSize: '12px' }}>0</span>
                        ) : '—'}
                      </td>
                      <td className="px-3 py-2.5">
                        {r.status === 'done' ? (
                          <span
                            className="flex items-center gap-1 text-[11.5px]"
                            style={{ color: 'var(--green)' }}
                          >
                            <CheckCircle className="h-3.5 w-3.5" />
                            done
                          </span>
                        ) : (
                          <span
                            className="flex items-center gap-1 text-[11px]"
                            style={{ color: 'var(--red)' }}
                            title={r.error}
                          >
                            <XCircle className="h-3.5 w-3.5 flex-shrink-0" />
                            <span className="truncate" style={{ maxWidth: '240px' }}>
                              {r.error ?? 'error'}
                            </span>
                          </span>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
