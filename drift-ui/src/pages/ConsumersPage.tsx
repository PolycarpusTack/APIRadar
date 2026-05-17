import { Users } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import Badge from '../components/Badge'
import EmptyState from '../components/EmptyState'

const TABLE_COLS = ['Consumer', 'Team', 'Contact', 'Runtime', 'Subscriptions', 'Last Seen']

interface ConsumerRow {
  id: string
  name: string
  ownerTeam: string
  contact: string
  repoUrl: string
  subscriptions: number
  lastSeen: string | null
}

function ConsumerTable({ rows }: { rows: ConsumerRow[] }) {
  if (rows.length === 0) {
    return (
      <EmptyState
        icon={Users}
        title="No consumers registered"
        description="Register a consumer with the CLI so it appears here and receives blast-radius alerts when APIs it uses change."
        action={
          <code
            className="rounded-md px-3 py-1.5 text-[11.5px]"
            style={{
              background: 'var(--bg-raised)',
              border: '1px solid var(--border)',
              color: 'var(--teal)',
              fontFamily: 'var(--font-mono)',
            }}
          >
            drift register --service-id &lt;id&gt; --consumer-name &lt;name&gt;
          </code>
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
                style={{
                  background: 'var(--bg-raised)',
                  borderColor: 'var(--border)',
                  color: 'var(--text-3)',
                }}
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
              className="group transition-colors"
              style={{ borderBottom: '1px solid var(--border)' }}
            >
              <td
                className="px-3 py-2.5 font-medium group-hover:bg-[var(--bg-hover)]"
                style={{ color: 'var(--text-1)', fontSize: '12.5px' }}
              >
                {row.name}
              </td>
              <td
                className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                style={{ color: 'var(--text-2)', fontSize: '12.5px' }}
              >
                {row.ownerTeam || <span style={{ color: 'var(--text-dim)' }}>—</span>}
              </td>
              <td
                className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                style={{ fontFamily: 'var(--font-mono)', fontSize: '11.5px', color: 'var(--text-2)' }}
              >
                {row.contact || <span style={{ color: 'var(--text-dim)' }}>—</span>}
              </td>
              <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]">
                {row.repoUrl ? (
                  <a
                    href={row.repoUrl}
                    target="_blank"
                    rel="noreferrer"
                    className="text-[11.5px] underline decoration-dotted hover:no-underline"
                    style={{ color: 'var(--cobalt-mid)', fontFamily: 'var(--font-mono)' }}
                  >
                    {row.repoUrl.replace(/^https?:\/\//, '').slice(0, 32)}
                    {row.repoUrl.length > 40 ? '…' : ''}
                  </a>
                ) : (
                  <span style={{ color: 'var(--text-dim)', fontSize: '12px' }}>—</span>
                )}
              </td>
              <td className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]">
                <Badge variant={row.subscriptions > 0 ? 'cobalt' : 'neutral'}>
                  {row.subscriptions}
                </Badge>
              </td>
              <td
                className="px-3 py-2.5 group-hover:bg-[var(--bg-hover)]"
                style={{ fontFamily: 'var(--font-mono)', fontSize: '11.5px', color: 'var(--text-3)' }}
              >
                {row.lastSeen ?? <span style={{ color: 'var(--text-dim)' }}>never</span>}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

export default function ConsumersPage() {
  const rows: ConsumerRow[] = []

  return (
    <div>
      <PageHeader
        tag="Registry"
        title="Consumers"
        description="Services that consume one or more of your APIs. Use drift register to add a consumer and start receiving blast-radius alerts."
      />

      <div className="px-14 py-8">
        <div
          className="overflow-hidden rounded-lg"
          style={{ border: '1px solid var(--border)', background: 'var(--bg-surface)' }}
        >
          <div className="flex items-center justify-between px-4 py-3">
            <p
              className="text-[11px] font-semibold uppercase tracking-[0.8px]"
              style={{ color: 'var(--text-3)' }}
            >
              All consumers
            </p>
          </div>
          <ConsumerTable rows={rows} />
        </div>
      </div>
    </div>
  )
}
