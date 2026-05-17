import React from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'

// Routing is wired in when drift-ui renderer is fully integrated (Story C-9).
// HashRouter will be added here at that point (required for file:// protocol).

const rootElement = document.getElementById('root')
if (!rootElement) {
  throw new Error('Root element not found')
}

createRoot(rootElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
)
