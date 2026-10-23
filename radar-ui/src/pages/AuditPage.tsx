import { useState } from 'react'
import { Link } from 'react-router-dom'
import { ChevronLeft, ChevronRight } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import Badge from '../components/Badge'
import { api } from '../lib/apiClient'
import { useFetch } from '../lib/useFetch'

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
  loading,
  onPrev,
  onNext,
  label,
}: {
  offset: number
  limit: number
  count: number
  loading: boolean
  onPrev: () => void
  onNext: () => void
  /** What is being paged, e.g. "policy decisions" — both pagers are on the
      same page, so the chevrons need distinct accessible names. */
  label: string
}) {
  const from = offset + 1
  const to = offset + count
  return (
    <div className="flex items-center justify-between px-4 py-2.5" style={{ borderTop: '1px solid var(--border)' }}>
      <p className="text-[11.5px]" style={{ color: 'var(--text-3)', fontFamily: 'var(--font-mono)' }}>
        {loading ? 'Loading…' : count === 0 ? 'No results' : `${from}–${to}`}
      </p>
      <div className="flex gap-1">
        <button
          type="button"
          onClick={onPrev}
          disabled={offset === 0}
          aria-label={`Previous page of ${label}`}
          className="rounded p-1 transition-colors hover:bg-[var(--bg-hover)]"
          style={{ color: offset === 0 ? 'var(--text-dim)' : 'var(--text-2)' }}
        >
          <ChevronLeft className="h-4 w-4" />
        </button>
        <button
          type="button"
          onClick={onNext}
          disabled={count < limit}
          aria-label={`Next page of ${label}`}
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
  const [decisionOffset, setDecisionOffset] = useState(0)
  const [ackOffset, setAckOffset] = useState(0)

  // N-23: both lists are offset-driven. `useFetch` aborts the request for the
  // page you just left, so clicking through pages faster than the API answers
  // can no longer repaint an older page over a newer one, and nothing is left
  // in flight after the page unmounts.
  const decisionsReq = useFetch<{ entries: PolicyDecision[] }>(
    (signal) => api.get(`/v1/policy-decisions?limit=${LIMIT}&offset=${decisionOffset}`, { signal }),
    [decisionOffset],
  )
  const acksReq = useFetch<{ entries: Acknowledgement[] }>(
    (signal) => api.get(`/v1/acknowledgements?limit=${LIMIT}&offset=${ackOffset}`, { signal }),
    [ackOffset],
  )

  const decisions = decisionsReq.data?.entries ?? []
  const acks = acksReq.data?.entries ?? []
  // Only the very first load blanks the table; later page-turns keep the
  // current page visible (and its controls usable) until the next one lands.
  const loadingDecisions = decisionsReq.loading && !decisionsReq.data
  const loadingAcks = acksReq.loading && !acksReq.data
  const errorDecisions = decisionsReq.error
  const errorAcks = acksReq.error

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
              <p role="alert" className="px-4 py-3 text-[12.5px]" style={{ color: 'var(--red)' }}>
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
                  loading={decisionsReq.loading}
                  onPrev={() => setDecisionOffset((o) => Math.max(0, o - LIMIT))}
                  onNext={() => setDecisionOffset((o) => o + LIMIT)}
                  label="policy decisions"
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
              <p role="alert" className="px-4 py-3 text-[12.5px]" style={{ color: 'var(--red)' }}>
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
                  loading={acksReq.loading}
                  onPrev={() => setAckOffset((o) => Math.max(0, o - LIMIT))}
                  onNext={() => setAckOffset((o) => o + LIMIT)}
                  label="acknowledgements"
                />
              </>
            )}
          </div>
        </section>
      </div>
    </div>
  )
}
