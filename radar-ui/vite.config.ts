import { defineConfig, loadEnv, type Plugin } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'

// Content-Security-Policy for the WEB build.
//
// It is injected at build time only (see cspPlugin below) rather than being
// hard-coded into index.html, because the Vite dev server + @vitejs/plugin-react
// inject an inline module script (the Fast Refresh preamble) and connect over a
// websocket for HMR — a strict CSP in the served index.html would break `pnpm dev`.
//
// Directives:
//   script-src 'self'            — only the hashed bundle chunks (no inline JS in the built HTML)
//   style-src  'unsafe-inline'   — the app renders with inline `style={{…}}` attributes
//              https://fonts.googleapis.com — the Google Fonts stylesheet in index.html
//   font-src   https://fonts.gstatic.com    — the Google Fonts woff2 files
//   connect-src 'self' (+ VITE_API_URL origin, if set to a cross-origin API)
//   frame-src  'self'            — the Scalar Playground runs in a sandboxed srcdoc <iframe>
//   img-src    'self' data:      — the SVG favicon and any data: URIs
function buildCsp(apiOrigin: string): string {
  const connectSrc = ["'self'", apiOrigin].filter(Boolean).join(' ')
  return [
    "default-src 'self'",
    "base-uri 'self'",
    "object-src 'none'",
    "script-src 'self'",
    "style-src 'self' 'unsafe-inline' https://fonts.googleapis.com",
    "font-src 'self' https://fonts.gstatic.com",
    "img-src 'self' data:",
    `connect-src ${connectSrc}`,
    "frame-src 'self'",
    "form-action 'self'",
  ].join('; ')
}

function cspPlugin(csp: string): Plugin {
  return {
    name: 'radar-inject-csp',
    apply: 'build',
    transformIndexHtml(html) {
      return html.replace(
        '<head>',
        `<head>\n    <meta http-equiv="Content-Security-Policy" content="${csp}" />`,
      )
    },
  }
}

export default defineConfig(({ command, mode }) => {
  const env = loadEnv(mode, process.cwd(), '')
  let apiOrigin = ''
  try {
    apiOrigin = env.VITE_API_URL ? new URL(env.VITE_API_URL).origin : ''
  } catch {
    apiOrigin = ''
  }

  return {
    // Production builds are served from /app/ by radar-api; dev server stays at root.
    base: command === 'build' ? '/app/' : '/',
    plugins: [react(), cspPlugin(buildCsp(apiOrigin))],
    resolve: {
      alias: {
        '@': path.resolve(__dirname, './src'),
      },
    },
    server: {
      port: 6173,
      proxy: {
        '/v1': 'http://localhost:17380',
        '/auth': 'http://localhost:17380',
        '/health': 'http://localhost:17380',
        '/scalar.js': 'http://localhost:17380',
        '/scalar': 'http://localhost:17380',
      },
    },
    build: {
      outDir: 'dist',
      emptyOutDir: true,
    },
  }
})
