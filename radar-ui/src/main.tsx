import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'
import App from './App'
import { initApiClient } from './lib/apiClient'
import './index.css'

const rootElement = document.getElementById('root')
if (!rootElement) {
  throw new Error('Root element not found')
}

const basename = (import.meta.env.BASE_URL ?? '/').replace(/\/$/, '')

// Resolve Electron sidecar URL before first render so all api.* calls use the
// correct absolute base.  In web mode this is a no-op (~0 ms).
void initApiClient().then(() => {
  createRoot(rootElement).render(
    <StrictMode>
      <BrowserRouter basename={basename}>
        <App />
      </BrowserRouter>
    </StrictMode>
  )
})
