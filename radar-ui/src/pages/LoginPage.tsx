import { LogIn } from 'lucide-react'
import logoUrl from '../assets/logo.png'

export default function LoginPage() {
  return (
    <div
      className="flex min-h-screen items-center justify-center"
      style={{ background: 'var(--bg-base)' }}
    >
      <div
        className="w-full max-w-sm rounded-xl p-8"
        style={{
          background: 'var(--bg-surface)',
          border: '1px solid var(--border)',
          boxShadow: '0 4px 24px 0 rgba(0,0,0,0.10)',
        }}
      >
        {/* Logo */}
        <div className="mb-8">
          <img
            src={logoUrl}
            alt="API Radar — Contract Monitor"
            style={{ height: '44px', width: 'auto', display: 'block' }}
          />
        </div>

        <h1
          className="text-[22px] font-bold tracking-[-0.5px] mb-2"
          style={{ fontFamily: 'var(--font-head)', color: 'var(--text-1)' }}
        >
          Sign in to Radar
        </h1>
        <p
          className="text-[13px] mb-8"
          style={{ color: 'var(--text-2)' }}
        >
          Use your organisation&apos;s identity provider to access the dashboard.
        </p>

        <a
          href="/auth/login"
          className="flex w-full items-center justify-center gap-2 rounded-lg px-4 py-2.5 text-[13.5px] font-semibold transition-all duration-[120ms]"
          style={{
            background: 'var(--cobalt)',
            color: 'var(--text-inverse)',
          }}
          onMouseEnter={(e) => {
            (e.currentTarget as HTMLAnchorElement).style.opacity = '0.88'
          }}
          onMouseLeave={(e) => {
            (e.currentTarget as HTMLAnchorElement).style.opacity = '1'
          }}
        >
          <LogIn className="h-4 w-4" />
          Continue with your identity provider
        </a>

        <p
          className="mt-6 text-center text-[11px]"
          style={{ color: 'var(--text-dim)' }}
        >
          Supports Google Workspace, Azure AD, Okta, and any OIDC provider.
        </p>
      </div>
    </div>
  )
}
