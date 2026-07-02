// Central API client for all radar-ui HTTP calls.
// Resolves base URL at startup (Electron IPC → VITE_API_URL → relative).
// Throws ApiError on non-2xx responses; returns undefined for 204 No Content.

export class ApiError extends Error {
  constructor(
    public readonly status: number,
    message: string,
    public readonly body?: unknown,
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

let _base = (
  (import.meta as { env?: { VITE_API_URL?: string } }).env?.VITE_API_URL ?? ''
).replace(/\/$/, '')

// Desktop-only per-session bearer token for the loopback sidecar. Resolved once
// via IPC in initApiClient() and attached to every request. Stays null in the
// plain web build (no window.drift), so web behavior is unchanged.
let _token: string | null = null

/**
 * Call once before the app renders.  In Electron production the renderer loads
 * from file://, so relative URLs fail.  window.drift.getApiUrl() returns the
 * sidecar's absolute URL via IPC; in web mode it is not defined and we keep the
 * VITE_API_URL / empty-string (same-origin) fallback.
 *
 * In desktop mode the sidecar requires an Authorization bearer on every /v1
 * request; window.drift.getApiToken() returns that per-session token via IPC.
 * In web mode it is not defined and no Authorization header is added.
 */
export async function initApiClient(): Promise<void> {
  const w = window as {
    drift?: {
      getApiUrl?: () => Promise<string>
      getApiToken?: () => Promise<string>
    }
  }
  if (w.drift?.getApiUrl) {
    try {
      const url = await w.drift.getApiUrl()
      _base = url.replace(/\/$/, '')
    } catch { /* keep env fallback */ }
  }
  if (w.drift?.getApiToken) {
    try {
      _token = await w.drift.getApiToken()
    } catch { /* no token — web build behavior */ }
  }
}

export function apiBase(): string {
  return _base
}

export interface FetchOptions {
  /** Bearer token for Authorization header (AI endpoints). */
  bearer?: string
  /** Pass 'include' for OIDC session cookie on /auth/* routes. */
  credentials?: RequestCredentials
  signal?: AbortSignal
}

async function request<T>(
  method: string,
  path: string,
  body?: unknown,
  opts: FetchOptions = {},
): Promise<T> {
  const headers: Record<string, string> = {}
  if (body !== undefined) headers['Content-Type'] = 'application/json'
  // Desktop sidecar session token, unless a per-call bearer overrides it.
  if (_token) headers['Authorization'] = `Bearer ${_token}`
  if (opts.bearer) headers['Authorization'] = `Bearer ${opts.bearer}`

  const res = await fetch(`${_base}${path}`, {
    method,
    headers,
    body: body !== undefined ? JSON.stringify(body) : undefined,
    credentials: opts.credentials,
    signal: opts.signal,
  })

  if (!res.ok) {
    let msg = res.statusText
    let parsedBody: unknown
    try {
      parsedBody = await res.json()
      const err = parsedBody as { error?: string; message?: string }
      msg = err.error ?? err.message ?? msg
    } catch { /* keep statusText */ }
    throw new ApiError(res.status, msg, parsedBody)
  }

  if (res.status === 204) return undefined as T
  return res.json() as Promise<T>
}

export const api = {
  get: <T>(path: string, opts?: FetchOptions) =>
    request<T>('GET', path, undefined, opts),
  post: <T>(path: string, body?: unknown, opts?: FetchOptions) =>
    request<T>('POST', path, body, opts),
  put: <T>(path: string, body?: unknown, opts?: FetchOptions) =>
    request<T>('PUT', path, body, opts),
  patch: <T>(path: string, body?: unknown, opts?: FetchOptions) =>
    request<T>('PATCH', path, body, opts),
  del: <T = void>(path: string, opts?: FetchOptions) =>
    request<T>('DELETE', path, undefined, opts),
}
