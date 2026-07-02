import { useEffect, useState } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { GitCompare, Plus, Rows, X, ChevronLeft, ChevronRight } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import Badge from '../components/Badge'
import EmptyState from '../components/EmptyState'
import CompareSpecsPanel from '../components/CompareSpecsPanel'
import BatchComparePanel from '../components/BatchComparePanel'
import { api, ApiError } from '../lib/apiClient'

interface DiffSummary {
  id: string
  service_id: string
  service_name: string
  from_git_ref: string
  to_git_ref: string
  pr_url: string | null
  created_at: string
  breaking_count: number
  risky_count: number
  safe_count: number
}

function formatDate(iso: string) {
  try {
    return new Date(iso).toLocaleDateString('en-GB', {
      day: '2-digit', month: 'short', year: 'numeric', hour: '2-digit', minute: '2-digit',
    })
  } catch {
    return iso
  }
}

function shortRef(ref: string) {
  return ref.length > 12 ? ref.slice(0, 12) : ref
}

const TABLE_COLS = ['Date', 'Service', 'Refs', 'Breaking', 'Risky', 'Safe']

function DiffTable({ rows, onSelect, onCompare }: { rows: DiffSummary[]; onSelect: (id: string) => void; onCompare: () => void }) {
  if (rows.length === 0) {
    return (
      <EmptyState
        icon={GitCompare}
        title="No diffs recorded yet"
        description="Use the Compare panel above to paste two specs directly, or run: radar check --base old.yaml --head new.yaml --api-url http://localhost:8080"
        action={
          <button
            onClick={onCompare}
            className="inline-flex items-center gap-1.5 rounded-md px-4 py-2 text-[12.5px] font-medium transition-opacity hover:opacity-90"
            style={{ background: 'var(--cobalt-mid)', color: '#fff' }}
          >
            <Plus className="h-3.5 w-3.5" />
            Compare specs
          </button>
        }
      />
    )
  }

  return (
    <div className="overflow-x-auto">
      <table className="w-full border-collapse">
        <thead>
          <tr>
            {TABLE_COLS.map((col) => (
              <th
                key={col}
                className="border-b px-3 py-2 text-left text-[10.5px] font-semibold uppercase tracking-[0.8px]"
                style={{ background: 'var(--bg-raised)', borderColor: 'var(--border)', color: 'var(--text-3)' }}
              >
                {col}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr
              key={row.id}
              className="group cursor-pointer transition-colors"
              style={{ borderBottom: '1px solid var(--border)' }}
              onClick={() => onSelect(row.id)}
            >
              <td
                className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                style={{ fontFamily: 'var(--font-mono)', fontSize: '11.5px', color: 'var(--text-3)' }}
              >
                {formatDate(row.created_at)}
              </td>
              <td
                className="px-3 py-2.5 font-medium group-hover:bg-[var(--bg-hover)]"
                style={{ fontSize: '12.5px', color: 'var(--text-1)' }}
              >
                {row.service_name}
              </td>
              <td
                className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                style={{ fontFamily: 'var(--font-mono)', fontSize: '11px', color: 'var(--text-3)' }}
              >
                <span style={{ color: 'var(--text-2)' }}>{shortRef(row.from_git_ref)}</span>
                <span className="mx-1">→</span>
                <span style={{ color: 'var(--cobalt-mid)' }}>{shortRef(row.to_git_ref)}</span>
              </td>
              <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]">
                {row.breaking_count > 0 ? (
                  <Badge variant="err">{row.breaking_count}</Badge>
                ) : (
                  <span style={{ color: 'var(--text-dim)', fontSize: '12px' }}>—</span>
                )}
              </td>
              <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]">
                {row.risky_count > 0 ? (
                  <Badge variant="warn">{row.risky_count}</Badge>
                ) : (
                  <span style={{ color: 'var(--text-dim)', fontSize: '12px' }}>—</span>
                )}
              </td>
              <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]">
                {row.safe_count > 0 ? (
                  <Badge variant="ok">{row.safe_count}</Badge>
                ) : (
                  <span style={{ color: 'var(--text-dim)', fontSize: '12px' }}>—</span>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

const PAGE_SIZE = 50
// When filtering by service we scan a wider recent window client-side, since
// GET /v1/diffs has no server-side service filter (API max page = 200).
const FILTER_WINDOW = 200

export default function DiffsPage() {
  const navigate = useNavigate()
  const [searchParams] = useSearchParams()
  const serviceFilter = searchParams.get('service')
  const [rows, setRows] = useState<DiffSummary[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [offset, setOffset] = useState(0)
  // Auto-open when navigated here with ?compare=open (e.g. from FirstRunBanner)
  const [showCompare, setShowCompare] = useState(searchParams.get('compare') === 'open')
  const [showBatch, setShowBatch] = useState(false)

  useEffect(() => {
    setLoading(true)
    setError(null)
    const query = serviceFilter
      ? `/v1/diffs?limit=${FILTER_WINDOW}&offset=0`
      : `/v1/diffs?limit=${PAGE_SIZE}&offset=${offset}`
    api.get<DiffSummary[]>(query)
      .then(setRows)
      .catch((e: unknown) => setError(e instanceof ApiError ? e.message : String(e)))
      .finally(() => setLoading(false))
  }, [offset, serviceFilter])

  // In filter mode we filter the fetched window client-side by service id.
  const visibleRows = serviceFilter ? rows.filter((r) => r.service_id === serviceFilter) : rows
  const filterName = serviceFilter ? (visibleRows[0]?.service_name ?? null) : null
  const hasNextPage = !serviceFilter && rows.length === PAGE_SIZE
  const hasPrevPage = !serviceFilter && offset > 0

  const countLabel = loading
    ? 'Loading…'
    : serviceFilter
      ? `${visibleRows.length} diff${visibleRows.length !== 1 ? 's' : ''}`
      : visibleRows.length === 0
        ? '0 diffs'
        : `Showing ${offset + 1}–${offset + visibleRows.length}`

  return (
    <div>
      <PageHeader
        tag="Monitor"
        title="Schema Diffs"
        description="Every drift check run posted to this server. Click a row to see the full blast-radius report."
      />

      <div className="px-14 py-8 flex flex-col gap-6">
        {/* Single compare panel */}
        {showCompare && (
          <CompareSpecsPanel onClose={() => setShowCompare(false)} />
        )}

        {/* Batch compare panel */}
        {showBatch && (
          <BatchComparePanel onClose={() => setShowBatch(false)} />
        )}

        <div
          className="overflow-hidden rounded-lg"
          style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}
        >
          <div className="flex items-center justify-between px-4 py-3" style={{ borderBottom: '1px solid var(--border)' }}>
            <div className="flex items-center gap-2.5">
              <p className="text-[11px] font-semibold uppercase tracking-[0.8px]" style={{ color: 'var(--text-3)' }}>
                {countLabel}
              </p>
              {serviceFilter && !loading && (
                <button
                  onClick={() => navigate('/diffs')}
                  className="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10.5px] font-medium transition-colors hover:opacity-80"
                  style={{ background: 'var(--bg-active)', color: 'var(--cobalt-mid)' }}
                  title="Clear service filter"
                >
                  {filterName ?? 'Filtered by service'}
                  <X className="h-2.5 w-2.5" />
                </button>
              )}
            </div>
            <div className="flex items-center gap-2">
              {!showCompare && (
                <button
                  onClick={() => { setShowCompare(true); setShowBatch(false) }}
                  className="flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-[12px] font-medium transition-colors hover:bg-[var(--bg-hover)]"
                  style={{ borderColor: 'var(--border-mid)', color: 'var(--text-2)' }}
                >
                  <Plus className="h-3.5 w-3.5" />
                  Compare Specs
                </button>
              )}
              {!showBatch && (
                <button
                  onClick={() => { setShowBatch(true); setShowCompare(false) }}
                  className="flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-[12px] font-medium transition-colors hover:bg-[var(--bg-hover)]"
                  style={{ borderColor: 'var(--border-mid)', color: 'var(--text-2)' }}
                >
                  <Rows className="h-3.5 w-3.5" />
                  Batch Compare
                </button>
              )}
            </div>
          </div>
          {error ? (
            <div className="px-4 py-3 text-[12.5px]" style={{ color: 'var(--red)' }}>
              Failed to load diffs: {error}
            </div>
          ) : (
            <>
              <DiffTable rows={visibleRows} onSelect={(id) => navigate(`/diffs/${id}`)} onCompare={() => { setShowCompare(true); setShowBatch(false) }} />
              {(hasPrevPage || hasNextPage) && (
                <div className="flex items-center justify-between px-4 py-3" style={{ borderTop: '1px solid var(--border)' }}>
                  <button
                    onClick={() => setOffset((o) => Math.max(0, o - PAGE_SIZE))}
                    disabled={!hasPrevPage}
                    className="inline-flex items-center gap-1 rounded-md border px-3 py-1.5 text-[12px] font-medium transition-colors hover:bg-[var(--bg-hover)] disabled:opacity-40 disabled:cursor-not-allowed"
                    style={{ borderColor: 'var(--border-mid)', color: 'var(--text-2)' }}
                  >
                    <ChevronLeft className="h-3.5 w-3.5" />
                    Previous
                  </button>
                  <button
                    onClick={() => setOffset((o) => o + PAGE_SIZE)}
                    disabled={!hasNextPage}
                    className="inline-flex items-center gap-1 rounded-md border px-3 py-1.5 text-[12px] font-medium transition-colors hover:bg-[var(--bg-hover)] disabled:opacity-40 disabled:cursor-not-allowed"
                    style={{ borderColor: 'var(--border-mid)', color: 'var(--text-2)' }}
                  >
                    Next
                    <ChevronRight className="h-3.5 w-3.5" />
                  </button>
                </div>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  )
}
