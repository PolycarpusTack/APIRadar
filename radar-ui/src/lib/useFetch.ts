// Shared abortable data-fetching hook.
//
// Replaces the ad-hoc `useEffect` + `useState` + `api.get(...).catch(() => {})`
// pattern that was duplicated across the high-traffic pages. Two problems that
// pattern had, both fixed here:
//
//   1. Swallowed errors — a failed request looked identical to an empty result.
//      `useFetch` exposes a distinct `error` string so pages can render an
//      honest error state.
//   2. Stale-response races — a slow response from a previous render (after
//      navigation/unmount, or a changed page/offset/filter) could land and
//      overwrite fresher state. `useFetch` aborts the in-flight request and
//      ignores any late resolution whenever the deps change or the component
//      unmounts.
//
// The fetcher receives an `AbortSignal`; pass it through to `api.get(path,
// { signal })` so the underlying `fetch` is actually cancelled.

import { useCallback, useEffect, useRef, useState, type DependencyList } from 'react'
import { ApiError } from './apiClient'

export interface UseFetchResult<T> {
  /** Latest successful payload, or `undefined` before the first success. */
  data: T | undefined
  /** True while a request is in flight. */
  loading: boolean
  /** Human-readable message when the last request failed, else `null`. */
  error: string | null
  /** Re-run the fetcher (e.g. after a mutation). Ordering-safe. */
  reload: () => void
}

/** Extract a user-facing message from an unknown thrown value. */
export function errorMessage(e: unknown): string {
  if (e instanceof ApiError) {
    return (e.body as { error?: string })?.error ?? e.message
  }
  if (e instanceof Error) return e.message
  return String(e)
}

/**
 * Run `fetcher` on mount and whenever `deps` change (or `reload()` is called),
 * cancelling any previous in-flight request so late responses can never land.
 *
 * @param fetcher Receives an AbortSignal — forward it to `api.get(path, { signal })`.
 * @param deps    Values that should trigger a refetch when they change.
 */
export function useFetch<T>(
  fetcher: (signal: AbortSignal) => Promise<T>,
  deps: DependencyList = [],
): UseFetchResult<T> {
  const [data, setData] = useState<T | undefined>(undefined)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [nonce, setNonce] = useState(0)

  // Keep the latest fetcher without making it a dep — callers pass an inline
  // closure that changes identity every render, so we drive refetches off
  // `deps` + `nonce` instead.
  const fetcherRef = useRef(fetcher)
  fetcherRef.current = fetcher

  useEffect(() => {
    const controller = new AbortController()
    let active = true
    setLoading(true)
    setError(null)

    fetcherRef.current(controller.signal)
      .then((result) => {
        if (active) setData(result)
      })
      .catch((e: unknown) => {
        if (!active || controller.signal.aborted) return
        if (e instanceof DOMException && e.name === 'AbortError') return
        setError(errorMessage(e))
      })
      .finally(() => {
        if (active) setLoading(false)
      })

    return () => {
      active = false
      controller.abort()
    }
    // `fetcher` is intentionally read via a ref (not a dep); refetches are
    // driven by `nonce` (reload) and the caller-supplied `deps`.
  }, [nonce, ...deps])

  const reload = useCallback(() => setNonce((n) => n + 1), [])

  return { data, loading, error, reload }
}
