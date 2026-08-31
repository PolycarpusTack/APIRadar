import { defineConfig, devices } from '@playwright/test';

// Full-stack e2e: same browser, same UI, but a REAL radar-api behind it.
//
// The default config (playwright.config.ts) drives e2e/, where every API call
// is mocked with page.route(). Those tests are fast and deterministic, and they
// stay — but they can only ever confirm the UI matches our fixtures. This
// config drives e2e-fullstack/, which mocks nothing, so a change to a response
// shape fails here instead of silently diverging from the mocks.
//
// The API is started by the caller (the `fullstack-e2e` CI job) rather than by
// `webServer`, because it needs a built binary, a database and migrations —
// more setup than a single command. Only the UI dev server is started here, and
// it proxies /v1 to the API, so the proxy config is exercised too.
export default defineConfig({
  testDir: './e2e-fullstack',
  // Sequential: these tests share one database, so parallel writes would make
  // failures depend on interleaving rather than on the code.
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  // No retries. A full-stack failure is a real integration problem worth
  // seeing; retrying would hide flakiness that matters here.
  retries: 0,
  reporter: process.env.CI ? 'list' : 'html',

  use: {
    baseURL: process.env.RADAR_UI_URL ?? 'http://localhost:6173',
    trace: 'retain-on-failure',
  },

  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],

  webServer: {
    command: 'pnpm dev',
    url: 'http://localhost:6173',
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
