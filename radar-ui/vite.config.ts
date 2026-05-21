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
      '/v1': 'http://localhost:8081',
      '/health': 'http://localhost:8081',
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  },
}))
