import { test, expect } from '@playwright/test';

// Journey (e): playground — API Explorer (Scalar) + CSV Data Runner.
// The paste-two-specs compare UI now lives on the Diffs page (CompareSpecsPanel).

test.describe('Playground journey', () => {
  test.beforeEach(async ({ page }) => {
    await page.route('**/auth/me', (r) => r.fulfill({ status: 503, json: {} }));
    await page.route('**/v1/readiness', (r) => r.fulfill({ json: { ok: true, items: [] } }));
  });

  test('navigates to Playground page', async ({ page }) => {
    await page.goto('/playground');
    await expect(page.locator('main')).toBeVisible();
    await expect(page).not.toHaveTitle(/error/i);
  });

  test('Playground page renders the API Explorer and mode tabs', async ({ page }) => {
    await page.goto('/playground');
    await expect(page.locator('main')).toBeVisible();
    // Default "API Explorer" mode renders the Scalar iframe; the CSV Data Runner
    // tab is also present.
    await expect(page.getByRole('button', { name: /API Explorer/i })).toBeVisible();
    await expect(page.getByRole('button', { name: /CSV Data Runner/i })).toBeVisible();
    await expect(page.locator('iframe[title="API Playground"]')).toBeVisible({
      timeout: 10_000,
    });
  });
});
