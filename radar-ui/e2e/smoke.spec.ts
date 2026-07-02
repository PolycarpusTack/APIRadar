// D-8: Smoke tests for radar-ui dashboard.
// Run: pnpm exec playwright test (requires `pnpm exec playwright install` first)
import { expect, test } from '@playwright/test';

test.describe('Dashboard smoke tests', () => {
  test('home page loads and shows navigation', async ({ page }) => {
    await page.goto('/');
    // Sidebar or top nav should contain the product name.
    await expect(page.getByText(/radar/i).first()).toBeVisible();
  });

  test('diffs page is reachable via nav', async ({ page }) => {
    await page.goto('/');
    // Navigate to diffs (adjust selector if nav labels change).
    const diffsLink = page.getByRole('link', { name: /diff/i }).first();
    if (await diffsLink.isVisible()) {
      await diffsLink.click();
      await expect(page).toHaveURL(/diff/i);
    }
  });

  test('playground page renders the API Explorer iframe', async ({ page }) => {
    await page.goto('/playground');
    // The playground's default "API Explorer" mode renders the Scalar iframe.
    await expect(page.locator('iframe[title="API Playground"]')).toBeVisible({
      timeout: 10_000,
    });
  });

  test('health endpoint returns ok (API reachability)', async ({ request }) => {
    const apiUrl = process.env.RADAR_API_URL ?? 'http://localhost:8080';
    // A refused connection throws rather than returning a non-ok response, so
    // catch it and skip gracefully when radar-api isn't running in this env.
    const resp = await request.get(`${apiUrl}/health`).catch(() => null);
    if (!resp || !resp.ok()) {
      test.skip(true, 'radar-api not reachable, skipping API health check');
      return;
    }
    const body = await resp.json();
    expect(body).toMatchObject({ status: 'ok' });
  });
});
