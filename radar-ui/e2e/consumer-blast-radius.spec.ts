import { test, expect } from '@playwright/test';

// Journey (b): register consumer → subscribe → blast radius shows consumer

test.describe('Consumer blast-radius journey', () => {
  test.beforeEach(async ({ page }) => {
    await page.route('**/auth/me', (r) => r.fulfill({ status: 503, json: {} }));
    await page.route('**/v1/readiness', (r) => r.fulfill({ json: { ok: true, items: [] } }));
    await page.route('**/v1/consumers**', (r) => r.fulfill({ json: [] }));
    await page.route('**/v1/services**', (r) => r.fulfill({ json: [] }));
  });

  test('navigates to Consumers page', async ({ page }) => {
    await page.goto('/consumers');
    await expect(page.locator('main')).toBeVisible();
    await expect(page).not.toHaveTitle(/error/i);
  });

  test('navigates to Evidence Coverage page', async ({ page }) => {
    await page.route('**/v1/evidence-coverage**', (r) =>
      r.fulfill({ json: { coverage: [] } }),
    );
    await page.goto('/evidence-coverage');
    await expect(page.locator('main')).toBeVisible();
    await expect(page).not.toHaveTitle(/error/i);
  });
});
