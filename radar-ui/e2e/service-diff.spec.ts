import { test, expect } from '@playwright/test';

// Journey (a): register service → compare specs → view diff with blast radius
// API responses are mocked so the test runs without a live radar-api instance.

test.describe('Service diff journey', () => {
  test.beforeEach(async ({ page }) => {
    // Mock the API endpoints this journey touches.
    await page.route('**/v1/readiness', (r) =>
      r.fulfill({ json: { ok: true, items: [] } }),
    );
    await page.route('**/v1/services', (r) =>
      r.fulfill({ json: [] }),
    );
    await page.route('**/v1/diffs**', (r) =>
      r.fulfill({ json: [] }),
    );
    await page.route('**/auth/me', (r) =>
      r.fulfill({ status: 503, json: {} }),
    );
  });

  test('navigates to Services and shows empty state', async ({ page }) => {
    await page.goto('/services');
    // The sidebar nav and main area must render without crashing.
    await expect(page.locator('nav')).toBeVisible();
    // Empty state or service list — whichever renders, the page must not error.
    await expect(page).not.toHaveTitle(/error/i);
  });

  test('navigates to Diffs page', async ({ page }) => {
    await page.goto('/diffs');
    await expect(page.locator('main')).toBeVisible();
    await expect(page).not.toHaveTitle(/error/i);
  });

  test('navigates to home page and overview renders', async ({ page }) => {
    await page.route('**/v1/services/summary**', (r) =>
      r.fulfill({ json: { services: 0, consumers: 0, breaking_changes: 0 } }),
    );
    await page.route('**/v1/consumers**', (r) => r.fulfill({ json: [] }));
    await page.goto('/');
    await expect(page.locator('main')).toBeVisible();
    await expect(page).not.toHaveTitle(/error/i);
  });
});
