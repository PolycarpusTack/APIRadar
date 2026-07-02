import { describe, it, expect } from 'vitest'
import { localDayKey, buildDiffBuckets, type DiffCounts } from './diffTimeline'

describe('localDayKey', () => {
  it('formats a date as local YYYY-MM-DD', () => {
    // Constructed from local components, so no timezone ambiguity.
    const d = new Date(2026, 6, 2, 9, 30) // 2 Jul 2026, local
    expect(localDayKey(d)).toBe('2026-07-02')
  })

  it('zero-pads month and day', () => {
    const d = new Date(2026, 0, 5, 0, 0)
    expect(localDayKey(d)).toBe('2026-01-05')
  })
})

describe('buildDiffBuckets', () => {
  const today = new Date(2026, 6, 10, 12, 0) // 10 Jul 2026, local

  it('returns one bucket per day, oldest first', () => {
    const buckets = buildDiffBuckets([], today, 30)
    expect(buckets).toHaveLength(30)
    expect(buckets[0].key).toBe('2026-06-11')
    expect(buckets[29].key).toBe('2026-07-10')
  })

  it('sums counts into the matching local day', () => {
    const diffs: DiffCounts[] = [
      { created_at: new Date(2026, 6, 10, 8, 0).toISOString(), breaking_count: 1, risky_count: 2, safe_count: 3 },
      { created_at: new Date(2026, 6, 10, 20, 0).toISOString(), breaking_count: 4, risky_count: 0, safe_count: 1 },
    ]
    const buckets = buildDiffBuckets(diffs, today, 30)
    const last = buckets[buckets.length - 1]
    expect(last.key).toBe('2026-07-10')
    expect(last).toMatchObject({ breaking: 5, risky: 2, safe: 4 })
  })

  it('ignores diffs outside the window', () => {
    const diffs: DiffCounts[] = [
      { created_at: new Date(2025, 0, 1, 12, 0).toISOString(), breaking_count: 9, risky_count: 9, safe_count: 9 },
    ]
    const buckets = buildDiffBuckets(diffs, today, 30)
    const totals = buckets.reduce((n, b) => n + b.breaking + b.risky + b.safe, 0)
    expect(totals).toBe(0)
  })

  it('buckets a late-evening local diff on its local day, not the UTC day', () => {
    // A diff at 23:30 local on 9 Jul; in a positive-offset zone its UTC date
    // would be 10 Jul. Bucketing by local day must keep it on 9 Jul.
    const localLate = new Date(2026, 6, 9, 23, 30)
    const diffs: DiffCounts[] = [
      { created_at: localLate.toISOString(), breaking_count: 1, risky_count: 0, safe_count: 0 },
    ]
    const buckets = buildDiffBuckets(diffs, today, 30)
    const byKey = Object.fromEntries(buckets.map((b) => [b.key, b]))
    expect(byKey['2026-07-09'].breaking).toBe(1)
    expect(byKey['2026-07-10'].breaking).toBe(0)
  })
})
