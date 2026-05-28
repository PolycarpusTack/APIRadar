import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'

export default defineConfig(({ command }) => ({
  // Production builds are served from /app/ by radar-api; dev server stays at root.
  base: command === 'build' ? '/app/' : '/',
  plugins: [react()],
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
}))
