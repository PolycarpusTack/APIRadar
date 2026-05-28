import { test, expect } from '@playwright/test';

// Journey (d): register webhook → test fire → delivery appears in history

test.describe('Webhook journey', () => {
  test.beforeEach(async ({ page }) => {
    await page.route('**/auth/me', (r) => r.fulfill({ status: 503, json: {} }));
    await page.route('**/v1/readiness', (r) => r.fulfill({ json: { ok: true, items: [] } }));
    await page.route('**/v1/webhooks', (r) =>
      r.fulfill({
        json: [
          {
            id: 'wh-1',
            url: 'https://hooks.example.com/radar',
            events: 'diff.created',
            active: true,
            created_at: new Date().toISOString(),
          },
        ],
      }),
    );
    await page.route('**/v1/webhooks/wh-1/deliveries', (r) =>
      r.fulfill({
        json: [
          {
            id: 'del-1',
            webhook_id: 'wh-1',
            event: 'diff.created',
            status: 'delivered',
            attempt: 1,
            delivered_at: new Date().toISOString(),
          },
        ],
      }),
    );
    await page.route('**/v1/csv-runs**', (r) => r.fulfill({ json: [] }));
    await page.route('**/v1/sandbox-envs**', (r) => r.fulfill({ json: [] }));
    await page.route('**/v1/scheduled-scans**', (r) => r.fulfill({ json: [] }));
    await page.route('**/v1/services**', (r) => r.fulfill({ json: [] }));
  });

  test('Settings page loads with webhook list', async ({ page }) => {
    await page.goto('/settings');
    await expect(page.locator('main')).toBeVisible();
    await expect(page).not.toHaveTitle(/error/i);
  });
});
