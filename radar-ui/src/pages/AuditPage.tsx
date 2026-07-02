import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { ChevronLeft, ChevronRight } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import Badge from '../components/Badge'
import { api, ApiError } from '../lib/apiClient'

interface PolicyDecision {
  id: string
  service_id: string | null
  diff_id: string | null
  verdict: string
  fail_mode: string
  actor: string | null
  created_at: string
}

interface Acknowledgement {
  id: string
  diff_id: string | null
  service_id: string | null
  consumer_id: string | null
  acknowledged_by: string
  reason: string | null
  expires_at: string | null
  created_at: string
}

function formatDate(iso: string) {
  try {
    return new Date(iso).toLocaleString('en-GB', {
      day: '2-digit', month: 'short', year: 'numeric', hour: '2-digit', minute: '2-digit',
    })
  } catch {
    return iso
  }
}

function verdictVariant(v: string): 'err' | 'warn' | 'ok' | 'neutral' {
  if (v === 'block') return 'err'
  if (v === 'overridden') return 'warn'
  if (v === 'pass') return 'ok'
  return 'neutral'
}

function TableHeader({ cols }: { cols: string[] }) {
  return (
    <thead>
      <tr>
        {cols.map((col) => (
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
  )
}

function Pagination({
  offset,
  limit,
  count,
  onPrev,
  onNext,
}: {
  offset: number
  limit: number
  count: number
  onPrev: () => void
  onNext: () => void
}) {
  const from = offset + 1
  const to = offset + count
  return (
    <div className="flex items-center justify-between px-4 py-2.5" style={{ borderTop: '1px solid var(--border)' }}>
      <p className="text-[11.5px]" style={{ color: 'var(--text-3)', fontFamily: 'var(--font-mono)' }}>
        {count === 0 ? 'No results' : `${from}–${to}`}
      </p>
      <div className="flex gap-1">
        <button
          onClick={onPrev}
          disabled={offset === 0}
          className="rounded p-1 transition-colors hover:bg-[var(--bg-hover)]"
          style={{ color: offset === 0 ? 'var(--text-dim)' : 'var(--text-2)' }}
        >
          <ChevronLeft className="h-4 w-4" />
        </button>
        <button
          onClick={onNext}
          disabled={count < limit}
          className="rounded p-1 transition-colors hover:bg-[var(--bg-hover)]"
          style={{ color: count < limit ? 'var(--text-dim)' : 'var(--text-2)' }}
        >
          <ChevronRight className="h-4 w-4" />
        </button>
      </div>
    </div>
  )
}

const LIMIT = 25

export default function AuditPage() {
  const [decisions, setDecisions] = useState<PolicyDecision[]>([])
  const [acks, setAcks] = useState<Acknowledgement[]>([])
  const [decisionOffset, setDecisionOffset] = useState(0)
  const [ackOffset, setAckOffset] = useState(0)
  const [loadingDecisions, setLoadingDecisions] = useState(true)
  const [loadingAcks, setLoadingAcks] = useState(true)
  const [errorDecisions, setErrorDecisions] = useState<string | null>(null)
  const [errorAcks, setErrorAcks] = useState<string | null>(null)

  useEffect(() => {
    setLoadingDecisions(true)
    api.get<{ entries: PolicyDecision[] }>(`/v1/policy-decisions?limit=${LIMIT}&offset=${decisionOffset}`)
      .then((data) => setDecisions(data.entries ?? []))
      .catch((e) => setErrorDecisions(e instanceof ApiError ? e.message : String(e)))
      .finally(() => setLoadingDecisions(false))
  }, [decisionOffset])

  useEffect(() => {
    setLoadingAcks(true)
    api.get<{ entries: Acknowledgement[] }>(`/v1/acknowledgements?limit=${LIMIT}&offset=${ackOffset}`)
      .then((data) => setAcks(data.entries ?? []))
      .catch((e) => setErrorAcks(e instanceof ApiError ? e.message : String(e)))
      .finally(() => setLoadingAcks(false))
  }, [ackOffset])

  return (
    <div>
      <PageHeader
        tag="Governance"
        title="Audit Trail"
        description="Every CI policy decision and manual acknowledgement is recorded here. Use this trail to review why a PR was blocked or overridden."
      />

      <div className="px-14 py-8 space-y-8">
        {/* Policy Decisions */}
        <section>
          <p className="mb-3 text-[9.5px] font-semibold uppercase tracking-[1.2px]" style={{ color: 'var(--text-dim)' }}>
            Policy Decisions
          </p>
          <div className="overflow-hidden rounded-lg" style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}>
            {loadingDecisions ? (
              <p className="px-4 py-6 text-center text-[12.5px]" style={{ color: 'var(--text-3)' }}>Loading…</p>
            ) : errorDecisions ? (
              <p className="px-4 py-3 text-[12.5px]" style={{ color: 'var(--red)' }}>
                Failed to load policy decisions: {errorDecisions}
              </p>
            ) : decisions.length === 0 ? (
              <p className="px-4 py-6 text-center text-[12.5px]" style={{ color: 'var(--text-3)' }}>
                No policy decisions recorded yet. Run <code style={{ fontFamily: 'var(--font-mono)' }}>radar check</code> or the GitHub Action to generate entries.
              </p>
            ) : (
              <>
                <table className="w-full border-collapse">
                  <TableHeader cols={['Verdict', 'Service', 'Diff', 'Fail Mode', 'Actor', 'Date']} />
                  <tbody>
                    {decisions.map((d) => (
                      <tr key={d.id} className="group" style={{ borderBottom: '1px solid var(--border)' }}>
                        <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]">
                          <Badge variant={verdictVariant(d.verdict)}>{d.verdict}</Badge>
                        </td>
                        <td
                          className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                          style={{ fontSize: '12.5px', color: 'var(--text-1)', fontFamily: 'var(--font-mono)' }}
                        >
                          {d.service_id ?? <span style={{ color: 'var(--text-dim)' }}>—</span>}
                        </td>
                        <td
                          className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                          style={{ fontSize: '11.5px', color: 'var(--cobalt-mid)', fontFamily: 'var(--font-mono)' }}
                        >
                          {d.diff_id ? (
                            <Link to={`/diffs/${d.diff_id}`} className="hover:underline">
                              {d.diff_id.slice(0, 8)}…
                            </Link>
                          ) : (
                            <span style={{ color: 'var(--text-dim)' }}>—</span>
                          )}
                        </td>
                        <td
                          className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                          style={{ fontSize: '12px', color: 'var(--text-2)' }}
                        >
                          {d.fail_mode}
                        </td>
                        <td
                          className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                          style={{ fontSize: '11.5px', color: 'var(--text-3)', fontFamily: 'var(--font-mono)' }}
                        >
                          {d.actor ?? <span style={{ color: 'var(--text-dim)' }}>—</span>}
                        </td>
                        <td
                          className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                          style={{ fontFamily: 'var(--font-mono)', fontSize: '11.5px', color: 'var(--text-3)' }}
                        >
                          {formatDate(d.created_at)}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                <Pagination
                  offset={decisionOffset}
                  limit={LIMIT}
                  count={decisions.length}
                  onPrev={() => setDecisionOffset((o) => Math.max(0, o - LIMIT))}
                  onNext={() => setDecisionOffset((o) => o + LIMIT)}
                />
              </>
            )}
          </div>
        </section>

        {/* Acknowledgements */}
        <section>
          <p className="mb-3 text-[9.5px] font-semibold uppercase tracking-[1.2px]" style={{ color: 'var(--text-dim)' }}>
            Acknowledgements
          </p>
          <div className="overflow-hidden rounded-lg" style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}>
            {loadingAcks ? (
              <p className="px-4 py-6 text-center text-[12.5px]" style={{ color: 'var(--text-3)' }}>Loading…</p>
            ) : errorAcks ? (
              <p className="px-4 py-3 text-[12.5px]" style={{ color: 'var(--red)' }}>
                Failed to load acknowledgements: {errorAcks}
              </p>
            ) : acks.length === 0 ? (
              <p className="px-4 py-6 text-center text-[12.5px]" style={{ color: 'var(--text-3)' }}>
                No acknowledgements recorded yet. Use the Diff detail page or the API to create acknowledgements.
              </p>
            ) : (
              <>
                <table className="w-full border-collapse">
                  <TableHeader cols={['Acknowledged By', 'Diff', 'Service', 'Consumer', 'Reason', 'Expires', 'Date']} />
                  <tbody>
                    {acks.map((a) => (
                      <tr key={a.id} className="group" style={{ borderBottom: '1px solid var(--border)' }}>
                        <td
                          className="px-3 py-2.5 font-medium group-hover:bg-[var(--bg-hover)]"
                          style={{ fontSize: '12.5px', color: 'var(--text-1)' }}
                        >
                          {a.acknowledged_by}
                        </td>
                        <td
                          className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                          style={{ fontSize: '11.5px', color: 'var(--cobalt-mid)', fontFamily: 'var(--font-mono)' }}
                        >
                          {a.diff_id ? (
                            <Link to={`/diffs/${a.diff_id}`} className="hover:underline">
                              {a.diff_id.slice(0, 8)}…
                            </Link>
                          ) : (
                            <span style={{ color: 'var(--text-dim)' }}>—</span>
                          )}
                        </td>
                        <td
                          className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                          style={{ fontSize: '11.5px', color: 'var(--text-2)', fontFamily: 'var(--font-mono)' }}
                        >
                          {a.service_id ?? <span style={{ color: 'var(--text-dim)' }}>—</span>}
                        </td>
                        <td
                          className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                          style={{ fontSize: '11.5px', color: 'var(--text-2)', fontFamily: 'var(--font-mono)' }}
                        >
                          {a.consumer_id ?? <span style={{ color: 'var(--text-dim)' }}>—</span>}
                        </td>
                        <td
                          className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                          style={{ fontSize: '12px', color: 'var(--text-3)', maxWidth: '200px' }}
                        >
                          <span className="truncate block" title={a.reason ?? ''}>
                            {a.reason ?? <span style={{ color: 'var(--text-dim)' }}>—</span>}
                          </span>
                        </td>
                        <td
                          className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                          style={{ fontFamily: 'var(--font-mono)', fontSize: '11.5px', color: 'var(--text-3)' }}
                        >
                          {a.expires_at ? formatDate(a.expires_at) : <span style={{ color: 'var(--text-dim)' }}>never</span>}
                        </td>
                        <td
                          className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                          style={{ fontFamily: 'var(--font-mono)', fontSize: '11.5px', color: 'var(--text-3)' }}
                        >
                          {formatDate(a.created_at)}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                <Pagination
                  offset={ackOffset}
                  limit={LIMIT}
                  count={acks.length}
                  onPrev={() => setAckOffset((o) => Math.max(0, o - LIMIT))}
                  onNext={() => setAckOffset((o) => o + LIMIT)}
                />
              </>
            )}
          </div>
        </section>
      </div>
    </div>
  )
}
