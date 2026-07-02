import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  test: {
    // Logic tests (*.test.ts) run in the fast Node environment. Component/page
    // tests (*.test.tsx) opt into jsdom with a `// @vitest-environment jsdom`
    // docblock at the top of the file, so the Node default stays untouched.
    environment: 'node',
    globals: true,
    include: ['src/**/*.test.{ts,tsx}'],
  },
})
