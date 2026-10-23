// The central HTTP client had no tests even though every page depends on its
// error shape (ApiError with the server's message), its 204 handling, and its
// AbortSignal pass-through (which is what makes `useFetch` cancellation real).
import { describe, it, expect, vi, afterEach } from 'vitest'
import { api, ApiError } from './apiClient'

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  })
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('apiClient error handling', () => {
  it('throws an ApiError carrying the status and the server error message', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => jsonResponse({ error: 'spec is invalid' }, 422)))

    const err = await api.get('/v1/diffs').catch((e: unknown) => e)

    expect(err).toBeInstanceOf(ApiError)
    expect((err as ApiError).status).toBe(422)
    expect((err as ApiError).message).toBe('spec is invalid')
    expect((err as ApiError).body).toEqual({ error: 'spec is invalid' })
  })

  it('falls back to the status text when the body is not JSON', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('<html>502</html>', { status: 502, statusText: 'Bad Gateway' })))

    const err = await api.get('/v1/diffs').catch((e: unknown) => e)

    expect(err).toBeInstanceOf(ApiError)
    expect((err as ApiError).status).toBe(502)
    expect((err as ApiError).message).toBe('Bad Gateway')
  })

  it('propagates an AbortError rather than converting it to an ApiError', async () => {
    const controller = new AbortController()
    vi.stubGlobal('fetch', vi.fn(async () => {
      throw new DOMException('The operation was aborted.', 'AbortError')
    }))
    controller.abort()

    const err = await api.get('/v1/diffs', { signal: controller.signal }).catch((e: unknown) => e)

    expect(err).toBeInstanceOf(DOMException)
    expect((err as DOMException).name).toBe('AbortError')
  })
})

describe('apiClient request shape', () => {
  it('returns undefined for 204 No Content (delete)', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(null, { status: 204 })))
    await expect(api.del('/v1/webhooks/wh-1')).resolves.toBeUndefined()
  })

  it('forwards the AbortSignal to fetch so cancellation actually reaches the network', async () => {
    const calls: RequestInit[] = []
    vi.stubGlobal('fetch', vi.fn(async (_url: string, init: RequestInit) => {
      calls.push(init)
      return jsonResponse([])
    }))
    const controller = new AbortController()

    await api.get('/v1/diffs', { signal: controller.signal })

    expect(calls[0].signal).toBe(controller.signal)
    expect(calls[0].method).toBe('GET')
    expect(calls[0].body).toBeUndefined()
  })

  it('sends a JSON body with a Content-Type header on POST/PUT/PATCH', async () => {
    const calls: RequestInit[] = []
    vi.stubGlobal('fetch', vi.fn(async (_url: string, init: RequestInit) => {
      calls.push(init)
      return jsonResponse({ ok: true })
    }))

    await api.post('/v1/webhooks', { url: 'https://example.com' })
    await api.put('/v1/settings', { retention_days: 90 })
    await api.patch('/v1/evolution-rules/r-1', { enabled: false })

    expect(calls.map(c => c.method)).toEqual(['POST', 'PUT', 'PATCH'])
    for (const call of calls) {
      expect((call.headers as Record<string, string>)['Content-Type']).toBe('application/json')
    }
    expect(calls[0].body).toBe(JSON.stringify({ url: 'https://example.com' }))
  })
})
