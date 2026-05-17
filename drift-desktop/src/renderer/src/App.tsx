import { useState } from 'react'

// Minimal renderer stub for the Electron shell.
// Full UI sharing with drift-ui is wired in a later story.

export default function App() {
  const [apiUrl, setApiUrl] = useState<string | null>(null)
  const [checking, setChecking] = useState(false)

  async function handleCheckApi() {
    setChecking(true)
    try {
      const url = await window.drift.getApiUrl()
      setApiUrl(url)
      alert(`drift-api URL: ${url}`)
    } catch (err) {
      alert(`Failed to get API URL: ${String(err)}`)
    } finally {
      setChecking(false)
    }
  }

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        height: '100vh',
        background: '#0f0f14',
        color: '#e8e9f0',
        fontFamily: 'system-ui, sans-serif',
        gap: '1.5rem',
      }}
    >
      <h1 style={{ fontSize: '1.5rem', fontWeight: 700, margin: 0 }}>
        drift-desktop renderer
      </h1>
      <p style={{ color: '#8b8fa8', margin: 0, fontSize: '0.875rem' }}>
        Electron shell — full UI coming in a later story.
      </p>
      <button
        onClick={() => void handleCheckApi()}
        disabled={checking}
        style={{
          padding: '0.5rem 1.25rem',
          background: '#3805e3',
          color: '#fff',
          border: 'none',
          borderRadius: '6px',
          cursor: checking ? 'not-allowed' : 'pointer',
          opacity: checking ? 0.7 : 1,
          fontSize: '0.875rem',
          fontWeight: 500,
        }}
      >
        {checking ? 'Checking…' : 'Check API'}
      </button>
      {apiUrl && (
        <p style={{ color: '#0d9488', fontSize: '0.8rem', margin: 0 }}>
          API URL: {apiUrl}
        </p>
      )}
    </div>
  )
}
