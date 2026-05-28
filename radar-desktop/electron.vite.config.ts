import { resolve } from 'path'
import { defineConfig, externalizeDepsPlugin } from 'electron-vite'
import react from '@vitejs/plugin-react'
import type { Plugin } from 'vite'

/**
 * Strip `crossorigin` attributes from every asset tag in the built HTML.
 *
 * Vite emits `crossorigin` on <script type="module"> and <link rel="stylesheet">
 * tags so browsers can enforce CORS for code-split chunks.  Over file:// (packaged
 * Electron) Chromium treats each directory as a distinct origin and therefore blocks
 * those requests — producing a blank screen.  Removing the attribute reverts to the
 * simple, same-context fetch that works correctly under file://.
 */
function removeCrossoriginPlugin(): Plugin {
  return {
    name: 'electron-remove-crossorigin',
    enforce: 'post',
    // Runs only during a production build, not in the dev server.
    apply: 'build',
    transformIndexHtml(html: string): string {
      // Remove bare `crossorigin` and `crossorigin="..."` in any form.
      return html.replace(/\s+crossorigin(?:="[^"]*")?/g, '')
    },
  }
}

export default defineConfig({
  main: {
    plugins: [externalizeDepsPlugin()],
    build: {
      lib: {
        entry: resolve(__dirname, 'electron/main/index.ts'),
      },
    },
  },
  preload: {
    plugins: [externalizeDepsPlugin()],
    build: {
      lib: {
        entry: resolve(__dirname, 'electron/preload/index.ts'),
      },
    },
  },
  renderer: {
    root: resolve(__dirname, 'src/renderer'),
    build: {
      rollupOptions: {
        input: resolve(__dirname, 'src/renderer/index.html'),
      },
    },
    resolve: {
      alias: {
        '@': resolve(__dirname, 'src/renderer/src'),
        '@radar-ui': resolve(__dirname, '../radar-ui/src'),
      },
    },
    plugins: [react(), removeCrossoriginPlugin()],
    server: {
      port: 5181,
      proxy: {
        '/v1': 'http://127.0.0.1:17380',
        '/health': 'http://127.0.0.1:17380',
        '/scalar.js': 'http://127.0.0.1:17380',
        '/scalar': 'http://127.0.0.1:17380',
      },
    },
  },
})
