import { NavLink, Routes, Route } from 'react-router-dom'
import { LayoutDashboard, GitCompare, Users, FileText, Telescope, Settings } from 'lucide-react'
import HomePage from './pages/HomePage'
import DiffsPage from './pages/DiffsPage'
import DiffDetailPage from './pages/DiffDetailPage'
import ConsumersPage from './pages/ConsumersPage'
import ReleaseNotesPage from './pages/ReleaseNotesPage'
import PlaygroundPage from './pages/PlaygroundPage'

const NAV = [
  {
    label: 'Monitor',
    items: [
      { to: '/', label: 'Overview', icon: LayoutDashboard, end: true },
      { to: '/diffs', label: 'Diffs', icon: GitCompare },
    ],
  },
  {
    label: 'Registry',
    items: [
      { to: '/consumers', label: 'Consumers', icon: Users },
    ],
  },
  {
    label: 'Docs & Demo',
    items: [
      { to: '/release-notes', label: 'Release Notes', icon: FileText },
      { to: '/playground', label: 'API Playground', icon: Telescope },
    ],
  },
]

function NavItem({
  to,
  label,
  icon: Icon,
  end,
}: {
  to: string
  label: string
  icon: typeof LayoutDashboard
  end?: boolean
}) {
  return (
    <NavLink
      to={to}
      end={end}
      className={({ isActive }) =>
        [
          'relative flex items-center gap-2 rounded-md px-[18px] py-[7px] mx-[10px] my-px',
          'text-[12.5px] font-medium transition-all duration-[120ms] select-none',
          isActive
            ? 'text-[var(--cobalt-mid)] bg-[var(--bg-active)]'
            : 'text-[var(--text-2)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-1)]',
        ].join(' ')
      }
    >
      {({ isActive }) => (
        <>
          {isActive && (
            <span
              className="absolute top-1/2 -translate-y-1/2 w-[3px] h-4 rounded-r"
              style={{ left: '-10px', background: 'var(--cobalt)' }}
              aria-hidden
            />
          )}
          <Icon className="h-[15px] w-[15px] flex-shrink-0" />
          {label}
        </>
      )}
    </NavLink>
  )
}

function Sidebar() {
  return (
    <aside
      className="fixed inset-y-0 left-0 z-[100] flex w-64 flex-col overflow-y-auto"
      style={{ background: 'var(--bg-surface)', borderRight: '1px solid var(--border)' }}
    >
      {/* Logo */}
      <div className="px-5 py-6" style={{ borderBottom: '1px solid var(--border)' }}>
        <div className="flex items-center gap-2.5 mb-1">
          <div
            className="relative flex-shrink-0 h-8 w-8 overflow-hidden rounded-[7px]"
            style={{ background: 'linear-gradient(135deg, var(--cobalt) 0%, var(--cobalt-mid) 100%)' }}
          >
            <div className="absolute inset-[6px] rounded-[3px] bg-white" />
          </div>
          <span
            className="text-[17px] font-bold tracking-[-0.4px]"
            style={{ fontFamily: 'var(--font-head)', color: 'var(--text-1)' }}
          >
            Drift Monitor
          </span>
        </div>
        <p
          className="text-[10px] uppercase tracking-[0.8px] pl-[42px]"
          style={{ fontFamily: 'var(--font-mono)', color: 'var(--text-3)' }}
        >
          API Contract
        </p>
      </div>

      {/* Navigation */}
      <nav className="flex-1 py-3">
        {NAV.map((section) => (
          <div key={section.label} className="py-3" style={{ borderBottom: '1px solid var(--border)' }}>
            <p
              className="px-5 pb-1 text-[9.5px] font-semibold uppercase tracking-[1.2px]"
              style={{ color: 'var(--text-dim)' }}
            >
              {section.label}
            </p>
            {section.items.map((item) => (
              <NavItem key={item.to} {...item} />
            ))}
          </div>
        ))}
      </nav>

      {/* Footer */}
      <div className="px-5 py-4" style={{ borderTop: '1px solid var(--border)' }}>
        <NavLink
          to="/settings"
          className="flex items-center gap-2 text-[12px] transition-colors hover:text-[var(--text-1)]"
          style={{ color: 'var(--text-3)' }}
        >
          <Settings className="h-3.5 w-3.5" />
          Settings
        </NavLink>
        <p
          className="mt-2 text-[10px]"
          style={{ fontFamily: 'var(--font-mono)', color: 'var(--text-dim)' }}
        >
          v0.1.0
        </p>
      </div>
    </aside>
  )
}

export default function App() {
  return (
    <div className="flex min-h-screen" style={{ background: 'var(--bg-base)' }}>
      <Sidebar />
      <main className="ml-64 flex-1">
        <Routes>
          <Route path="/" element={<HomePage />} />
          <Route path="/diffs" element={<DiffsPage />} />
          <Route path="/diffs/:id" element={<DiffDetailPage />} />
          <Route path="/consumers" element={<ConsumersPage />} />
          <Route path="/release-notes" element={<ReleaseNotesPage />} />
          <Route path="/playground" element={<PlaygroundPage />} />
        </Routes>
      </main>
    </div>
  )
}
