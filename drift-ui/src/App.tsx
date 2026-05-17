import { Routes, Route, NavLink } from 'react-router-dom'
import HomePage from './pages/HomePage'
import DiffsPage from './pages/DiffsPage'
import ConsumersPage from './pages/ConsumersPage'

function Sidebar() {
  const navItems = [
    { to: '/', label: 'Overview' },
    { to: '/diffs', label: 'Diffs' },
    { to: '/consumers', label: 'Consumers' },
  ]

  return (
    <aside className="w-56 flex-shrink-0 flex flex-col bg-[#0a0a0f] border-r border-white/10 h-screen">
      <div className="px-5 py-5 border-b border-white/10">
        <span className="text-xs font-semibold tracking-widest text-[var(--text-dim)] uppercase">
          Drift Monitor
        </span>
      </div>
      <nav className="flex-1 px-3 py-4 space-y-1">
        {navItems.map(({ to, label }) => (
          <NavLink
            key={to}
            to={to}
            end={to === '/'}
            className={({ isActive }) =>
              [
                'flex items-center gap-3 px-3 py-2 rounded-md text-sm font-medium transition-colors',
                isActive
                  ? 'bg-[var(--cobalt)] text-white'
                  : 'text-[var(--text-dim)] hover:text-white hover:bg-white/5',
              ].join(' ')
            }
          >
            {label}
          </NavLink>
        ))}
      </nav>
      <div className="px-5 py-4 border-t border-white/10">
        <span className="text-xs text-[var(--text-dim)]">v0.1.0</span>
      </div>
    </aside>
  )
}

export default function App() {
  return (
    <div className="flex h-screen overflow-hidden bg-[var(--bg-base)] text-white">
      <Sidebar />
      <main className="flex-1 overflow-y-auto">
        <Routes>
          <Route path="/" element={<HomePage />} />
          <Route path="/diffs" element={<DiffsPage />} />
          <Route path="/consumers" element={<ConsumersPage />} />
        </Routes>
      </main>
    </div>
  )
}
