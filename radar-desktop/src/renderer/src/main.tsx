import React from 'react'
import { createRoot } from 'react-dom/client'
import { HashRouter } from 'react-router-dom'
import App from '@radar-ui/App'
import '@radar-ui/index.css'

// In production Electron the page is served from file:// so relative fetch
// paths like /v1/services never reach the sidecar.  Rewrite them here.
// In dev, window.location.protocol is http: (Vite dev server proxy handles it).
if (window.location.protocol === 'file:') {
  const API_BASE = 'http://127.0.0.1:17380'
  const _fetch = window.fetch.bind(window)
  window.fetch = (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    if (typeof input === 'string' && input.startsWith('/')) {
      return _fetch(API_BASE + input, init)
    }
    if (input instanceof Request && input.url.startsWith('/')) {
      return _fetch(new Request(API_BASE + input.url, input), init)
    }
    return _fetch(input, init)
  }
}

const rootElement = document.getElementById('root')
if (!rootElement) {
  throw new Error('Root element not found')
}

createRoot(rootElement).render(
  <React.StrictMode>
    <HashRouter future={{ v7_startTransition: true, v7_relativeSplatPath: true }}>
      <App />
    </HashRouter>
  </React.StrictMode>
)
