import { NavLink, Routes, Route, Navigate } from 'react-router-dom'
import { useEffect, useState } from 'react'
import { LayoutDashboard, GitCompare, Users, FileText, Telescope, FlaskConical, HelpCircle, Settings, Server, LogOut } from 'lucide-react'
import HomePage from './pages/HomePage'
import DiffsPage from './pages/DiffsPage'
import DiffDetailPage from './pages/DiffDetailPage'
import ConsumersPage from './pages/ConsumersPage'
import ConsumerDetailPage from './pages/ConsumerDetailPage'
import ServicesPage from './pages/ServicesPage'
import ReleaseNotesPage from './pages/ReleaseNotesPage'
import PlaygroundPage from './pages/PlaygroundPage'
import GenerateTestsPage from './pages/GenerateTestsPage'
import HelpPage from './pages/HelpPage'
import SettingsPage from './pages/SettingsPage'
import LoginPage from './pages/LoginPage'

// ---------------------------------------------------------------------------
// D-4: OIDC auth hook
// ---------------------------------------------------------------------------

interface AuthState {
  /** null = loading, false = unauthenticated / OIDC not configured, object = authenticated */
  session: { sub: string; org_id: string } | null | false
}

function useAuth(): AuthState {
  const [session, setSession] = useState<AuthState['session']>(null)

  useEffect(() => {
    fetch('/auth/me', { credentials: 'include' })
      .then(async (res) => {
        if (res.ok) {
          const data = await res.json()
          setSession(data)
        } else if (res.status === 401) {
          // OIDC configured but not authenticated
          setSession(false)
        } else {
          // 503 = OIDC not configured — let the app run unauthenticated
          setSession(null)
        }
      })
      .catch(() => {
        // Network error or OIDC endpoint absent — allow unauthenticated access
        setSession(null)
      })
  }, [])

  return { session }
}

// ---------------------------------------------------------------------------
// Navigation structure
// ---------------------------------------------------------------------------

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
      { to: '/services', label: 'Services', icon: Server },
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
  {
    label: 'Testing',
    items: [
      { to: '/generate-tests', label: 'Generate Tests', icon: FlaskConical },
    ],
  },
  {
    label: 'Help',
    items: [
      { to: '/help', label: 'Help & Reference', icon: HelpCircle },
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

function Sidebar({ showSignOut }: { showSignOut: boolean }) {
  function handleSignOut() {
    // Navigate to the logout endpoint; the server will clear the cookie and
    // redirect to /app/login. We let the browser follow the redirect.
    window.location.href = '/auth/logout'
  }

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
            <div className="absolute inset-[6px] rounded-[3px]" style={{ background: 'var(--text-inverse)' }} />
          </div>
          <span
            className="text-[17px] font-bold tracking-[-0.4px]"
            style={{ fontFamily: 'var(--font-head)', color: 'var(--text-1)' }}
          >
            Radar Monitor
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
        {showSignOut && (
          <button
            onClick={handleSignOut}
            className="mt-2 flex items-center gap-2 text-[12px] transition-colors hover:text-[var(--text-1)] w-full text-left"
            style={{ color: 'var(--text-3)', background: 'none', border: 'none', cursor: 'pointer', padding: 0 }}
          >
            <LogOut className="h-3.5 w-3.5" />
            Sign out
          </button>
        )}
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
  const { session } = useAuth()

  // session === false  → OIDC is configured, user is not authenticated
  // session === null   → still loading or OIDC not configured (allow through)
  // session === object → authenticated

  // Render login page for unauthenticated visits when OIDC is active.
  // While loading (null) we fall through to the main layout to avoid flash.
  if (session === false) {
    return (
      <Routes>
        <Route path="/login" element={<LoginPage />} />
        <Route path="*" element={<Navigate to="/login" replace />} />
      </Routes>
    )
  }

  const showSignOut = !!session

  return (
    <div className="flex min-h-screen" style={{ background: 'var(--bg-base)' }}>
      <Sidebar showSignOut={showSignOut} />
      <main className="ml-64 flex-1">
        <Routes>
          <Route path="/" element={<HomePage />} />
          <Route path="/diffs" element={<DiffsPage />} />
          <Route path="/diffs/:id" element={<DiffDetailPage />} />
          <Route path="/services" element={<ServicesPage />} />
          <Route path="/consumers" element={<ConsumersPage />} />
          <Route path="/consumers/:id" element={<ConsumerDetailPage />} />
          <Route path="/release-notes" element={<ReleaseNotesPage />} />
          <Route path="/playground" element={<PlaygroundPage />} />
          <Route path="/generate-tests" element={<GenerateTestsPage />} />
          <Route path="/help" element={<HelpPage />} />
          <Route path="/settings" element={<SettingsPage />} />
          <Route path="/login" element={<Navigate to="/" replace />} />
        </Routes>
      </main>
    </div>
  )
}
