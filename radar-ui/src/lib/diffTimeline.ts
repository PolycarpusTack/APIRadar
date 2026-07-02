// Pure helpers for the HomePage "Diff activity" timeline.
//
// The diffs table (DiffsPage) formats timestamps in the viewer's local time.
// To keep a single diff from appearing under different calendar days across the
// two views, the timeline buckets diffs by *local* calendar day as well —
// rather than the UTC date derived from the raw ISO string prefix.

export interface DiffCounts {
  created_at: string
  breaking_count: number
  risky_count: number
  safe_count: number
}

export interface DayBucket {
  key: string
  breaking: number
  risky: number
  safe: number
}

/** YYYY-MM-DD for the given date in the viewer's local timezone. */
export function localDayKey(d: Date): string {
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${y}-${m}-${day}`
}

/**
 * Bucket diffs into `days` consecutive local-day buckets ending on `today`.
 * Buckets are returned oldest-first. Diffs outside the window are ignored.
 */
export function buildDiffBuckets(diffs: DiffCounts[], today: Date, days: number): DayBucket[] {
  const end = new Date(today)
  end.setHours(23, 59, 59, 999)

  const buckets = new Map<string, DayBucket>()
  for (let i = days - 1; i >= 0; i--) {
    const d = new Date(end)
    d.setDate(d.getDate() - i)
    const key = localDayKey(d)
    buckets.set(key, { key, breaking: 0, risky: 0, safe: 0 })
  }

  for (const diff of diffs) {
    const key = localDayKey(new Date(diff.created_at))
    const bucket = buckets.get(key)
    if (bucket) {
      bucket.breaking += diff.breaking_count
      bucket.risky += diff.risky_count
      bucket.safe += diff.safe_count
    }
  }

  return Array.from(buckets.values())
}
