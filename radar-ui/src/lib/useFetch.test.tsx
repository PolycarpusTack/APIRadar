// @vitest-environment jsdom
//
// `useFetch` is the most load-bearing piece of the data layer — every migrated
// page depends on it for cancellation and honest error states — and it had no
// tests. These cover the three guarantees pages rely on:
//
//   1. the in-flight request is aborted when the component unmounts
//   2. the in-flight request is aborted when the deps (the "key") change, and a
//      late response from the previous key can never overwrite the newer data
//   3. a rejection surfaces as a distinct `error` string, not an empty result
import '@testing-library/jest-dom/vitest'
import { describe, it, expect, afterEach } from 'vitest'
import { render, screen, act, cleanup, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useFetch, errorMessage } from './useFetch'
import { ApiError } from './apiClient'

interface Deferred<T> {
  promise: Promise<T>
  resolve: (value: T) => void
  reject: (reason: unknown) => void
}

function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((res, rej) => { resolve = res; reject = rej })
  return { promise, resolve, reject }
}

function Probe({
  fetcher,
  fetchKey,
}: {
  fetcher: (signal: AbortSignal) => Promise<string>
  fetchKey: string
}) {
  const { data, loading, error, reload } = useFetch(fetcher, [fetchKey])
  return (
    <div>
      <span data-testid="state">
        {loading ? 'loading' : error ? `error:${error}` : `data:${data ?? 'empty'}`}
      </span>
      <span data-testid="data">{data ?? ''}</span>
      <button type="button" onClick={reload}>reload</button>
    </div>
  )
}

afterEach(() => cleanup())

describe('useFetch cancellation', () => {
  it('aborts the in-flight request when the component unmounts', async () => {
    const pending = deferred<string>()
    const signals: AbortSignal[] = []
    const fetcher = (signal: AbortSignal) => { signals.push(signal); return pending.promise }

    const { unmount } = render(<Probe fetcher={fetcher} fetchKey="a" />)
    expect(signals).toHaveLength(1)
    expect(signals[0].aborted).toBe(false)

    unmount()

    expect(signals[0].aborted).toBe(true)

    // A late resolution after unmount must not throw or update anything.
    await act(async () => { pending.resolve('LATE'); await pending.promise })
  })

  it('aborts the previous request on a key change and ignores its late response', async () => {
    const first = deferred<string>()
    const second = deferred<string>()
    const signals: AbortSignal[] = []
    const fetcher = (signal: AbortSignal) => {
      signals.push(signal)
      return signals.length === 1 ? first.promise : second.promise
    }

    const { rerender } = render(<Probe fetcher={fetcher} fetchKey="a" />)
    rerender(<Probe fetcher={fetcher} fetchKey="b" />)

    expect(signals).toHaveLength(2)
    expect(signals[0].aborted).toBe(true)
    expect(signals[1].aborted).toBe(false)

    // The newer key resolves first…
    await act(async () => { second.resolve('B'); await second.promise })
    expect(screen.getByTestId('state')).toHaveTextContent('data:B')

    // …and the stale response from key "a" lands afterwards. It must not win.
    await act(async () => { first.resolve('A'); await first.promise })
    expect(screen.getByTestId('state')).toHaveTextContent('data:B')
  })

  it('does not refetch when only the fetcher closure identity changes', async () => {
    let calls = 0
    const { rerender } = render(
      <Probe fetcher={async () => { calls += 1; return 'X' }} fetchKey="a" />,
    )
    await waitFor(() => expect(screen.getByTestId('state')).toHaveTextContent('data:X'))
    // Same key, brand-new closure — must not trigger a second request.
    rerender(<Probe fetcher={async () => { calls += 1; return 'X' }} fetchKey="a" />)
    expect(calls).toBe(1)
  })

  it('refetches on reload()', async () => {
    let calls = 0
    render(<Probe fetcher={async () => { calls += 1; return `X${calls}` }} fetchKey="a" />)
    await waitFor(() => expect(screen.getByTestId('state')).toHaveTextContent('data:X1'))
    await userEvent.click(screen.getByRole('button', { name: 'reload' }))
    await waitFor(() => expect(screen.getByTestId('state')).toHaveTextContent('data:X2'))
  })
})

describe('useFetch error state', () => {
  it('surfaces a rejection as an error distinct from an empty result', async () => {
    render(
      <Probe fetcher={async () => { throw new Error('boom') }} fetchKey="a" />,
    )
    await waitFor(() => expect(screen.getByTestId('state')).toHaveTextContent('error:boom'))
    expect(screen.getByTestId('data')).toHaveTextContent('')
  })

  it('prefers the server-supplied message on an ApiError', async () => {
    render(
      <Probe
        fetcher={async () => { throw new ApiError(422, 'Unprocessable', { error: 'spec is invalid' }) }}
        fetchKey="a"
      />,
    )
    await waitFor(() => expect(screen.getByTestId('state')).toHaveTextContent('error:spec is invalid'))
  })

  it('never reports an abort as an error', async () => {
    const pending = deferred<string>()
    const { unmount, rerender } = render(
      <Probe
        fetcher={(signal) => {
          signal.addEventListener('abort', () => pending.reject(new DOMException('aborted', 'AbortError')))
          return pending.promise
        }}
        fetchKey="a"
      />,
    )
    await act(async () => {
      rerender(
        <Probe fetcher={async () => 'B'} fetchKey="b" />,
      )
    })
    await waitFor(() => expect(screen.getByTestId('state')).toHaveTextContent('data:B'))
    unmount()
  })

  it('clears a previous error when a later request succeeds', async () => {
    let calls = 0
    render(
      <Probe
        fetcher={async () => {
          calls += 1
          if (calls === 1) throw new Error('boom')
          return 'OK'
        }}
        fetchKey="a"
      />,
    )
    await waitFor(() => expect(screen.getByTestId('state')).toHaveTextContent('error:boom'))
    await userEvent.click(screen.getByRole('button', { name: 'reload' }))
    await waitFor(() => expect(screen.getByTestId('state')).toHaveTextContent('data:OK'))
  })
})

describe('errorMessage', () => {
  it('unwraps ApiError bodies, Errors, and unknown throws', () => {
    expect(errorMessage(new ApiError(500, 'Internal', { error: 'db down' }))).toBe('db down')
    expect(errorMessage(new ApiError(500, 'Internal', {}))).toBe('Internal')
    expect(errorMessage(new Error('plain'))).toBe('plain')
    expect(errorMessage('a string')).toBe('a string')
  })
})
