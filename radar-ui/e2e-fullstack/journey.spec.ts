// Full-stack journey: a real radar-api, a real browser, no mocks.
//
// The existing specs in e2e/ mock every API response with page.route() — 28
// hand-written fixtures. That makes them fast and deterministic, but it means
// the UI is only ever checked against what we *believe* the API returns. If a
// response shape changes, the mocks keep passing while the real product breaks,
// and nothing in CI notices.
//
// That is not hypothetical. `fixtures/seed-demo.sh` — the README's five-minute
// demo, the first thing a new user runs — posted `{"events": [...]}` to two
// endpoints that take a bare array, and passed a consumer *name* where the
// handler wants a consumer *id*. Both silently failed, so the demo seeded
// services and specs but no evidence, and the blast radius came up empty:
// exactly the thing the demo exists to show. A mocked test cannot catch that.
// This one does, because it seeds through the same endpoints the demo uses and
// then asserts the UI renders the result.
//
// Runs against the UI dev server, which proxies /v1 to the API, so the proxy
// configuration is covered too.
import { expect, test, type APIRequestContext } from '@playwright/test';

const SERVICE_ID = 'payments-api';

async function seedDemoScenario(request: APIRequestContext) {
  // 1. The producer.
  const svc = await request.post('/v1/services', {
    data: {
      id: SERVICE_ID,
      name: 'Payments API',
      repo_url: 'https://github.com/example/payments-api',
      owner_team: 'platform',
      spec_format: 'openapi',
    },
  });
  expect(
    svc.ok() || svc.status() === 409,
    `service registration failed: ${svc.status()} ${await svc.text()}`,
  ).toBeTruthy();

  // 2. A consumer. The evidence endpoints key on the returned id, not the
  //    name — the bug the demo script had.
  const consumerResp = await request.post('/v1/consumers/upsert', {
    data: {
      name: 'billing-svc',
      owner_team: 'billing',
      contact: 'billing@example.com',
    },
  });
  expect(
    consumerResp.ok(),
    `consumer upsert failed: ${consumerResp.status()} ${await consumerResp.text()}`,
  ).toBeTruthy();
  const consumerId = (await consumerResp.json()).id as string;
  expect(consumerId, 'upsert must return a consumer id').toBeTruthy();

  // 3. Runtime evidence — a BARE ARRAY, which is what the handler accepts.
  const usage = await request.post('/v1/usage/events', {
    data: [
      {
        consumer_id: consumerId,
        service_id: SERVICE_ID,
        operation: 'GET /users/{id}',
        field_path: 'response.body.phone',
        observed_at: new Date().toISOString(),
      },
    ],
  });
  expect(
    usage.ok(),
    `usage ingest failed: ${usage.status()} ${await usage.text()}`,
  ).toBeTruthy();
  expect((await usage.json()).accepted).toBeGreaterThan(0);

  // 4. Static evidence — likewise a bare array.
  const callSites = await request.post('/v1/call-sites', {
    data: [
      {
        consumer_id: consumerId,
        service_id: SERVICE_ID,
        operation: 'GET /users/{id}',
        field_path: 'response.phone',
        file_path: 'src/clients/users.ts',
        line_number: 14,
      },
    ],
  });
  expect(
    callSites.ok(),
    `call-site ingest failed: ${callSites.status()} ${await callSites.text()}`,
  ).toBeTruthy();

  return { consumerId };
}

test.describe('full-stack: real API, no mocks', () => {
  test('the API is actually running and reachable through the UI proxy', async ({
    request,
  }) => {
    const health = await request.get('/v1/../health');
    expect(
      health.ok(),
      'the full-stack suite is pointless if it silently falls back to mocks',
    ).toBeTruthy();
  });

  test('demo scenario seeds through the real endpoints', async ({ request }) => {
    // This is the regression guard for the broken seed script: every call
    // asserts on the real status code, so a payload-shape change fails here
    // instead of quietly producing an empty dashboard.
    const { consumerId } = await seedDemoScenario(request);
    expect(consumerId).toMatch(/[0-9a-f-]{36}/);
  });

  test('a seeded consumer appears in the dashboard', async ({
    page,
    request,
  }) => {
    await seedDemoScenario(request);

    await page.goto('/consumers');
    await expect(
      page.getByText('billing-svc').first(),
      'a consumer seeded through the API must render in the UI',
    ).toBeVisible({ timeout: 15_000 });
  });

  test('a seeded service appears in the dashboard', async ({
    page,
    request,
  }) => {
    await seedDemoScenario(request);

    await page.goto('/services');
    await expect(page.getByText(/payments/i).first()).toBeVisible({
      timeout: 15_000,
    });
  });
});
