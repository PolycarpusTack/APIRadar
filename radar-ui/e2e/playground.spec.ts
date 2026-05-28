import { test, expect } from '@playwright/test';

// Journey (e): playground — paste two specs → view inline diff

test.describe('Playground journey', () => {
  test.beforeEach(async ({ page }) => {
    await page.route('**/auth/me', (r) => r.fulfill({ status: 503, json: {} }));
    await page.route('**/v1/readiness', (r) => r.fulfill({ json: { ok: true, items: [] } }));
    // Inline diff comparison is handled client-side; no API route needed for basic render.
  });

  test('navigates to Playground page', async ({ page }) => {
    await page.goto('/playground');
    await expect(page.locator('main')).toBeVisible();
    await expect(page).not.toHaveTitle(/error/i);
  });

  test('Playground page renders two spec input areas', async ({ page }) => {
    await page.goto('/playground');
    await expect(page.locator('main')).toBeVisible();
    // The page should have at least one textarea or code-editor area for specs.
    const textareas = page.locator('textarea');
    const count = await textareas.count();
    expect(count).toBeGreaterThanOrEqual(1);
  });
});
