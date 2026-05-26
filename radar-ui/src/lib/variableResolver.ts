import type { PlaygroundRequest } from './variableExtractor'

export interface ResolvedRequest {
  url: string
  method: string
  headers: Record<string, string>
  body: string
  /** Variables present in template but not found in CSV row */
  unresolved: string[]
}

const SECRET_PATTERNS = /authorization|token|key|secret|password|bearer/i

/** Replace {{var}} placeholders in `text` using `row`, tracking unresolved vars. */
function resolve(text: string, row: Record<string, string>, unresolved: Set<string>): string {
  return text.replace(/\{\{([a-zA-Z_][a-zA-Z0-9_]*)\}\}/g, (_, name: string) => {
    if (name in row) return row[name]
    unresolved.add(name)
    return `{{${name}}}`
  })
}

export function resolveRequest(
  template: PlaygroundRequest,
  row: Record<string, string>,
): ResolvedRequest {
  const unresolved = new Set<string>()

  const url = resolve(template.url, row, unresolved)
  const body = resolve(template.body, row, unresolved)
  const headers: Record<string, string> = {}
  for (const { key, value } of template.headers) {
    const resolvedKey = resolve(key, row, unresolved)
    const resolvedValue = resolve(value, row, unresolved)
    headers[resolvedKey] = resolvedValue
  }

  return { url, method: template.method, headers, body, unresolved: Array.from(unresolved) }
}

/** Mask header values whose names look like secrets. */
export function maskSecrets(headers: Record<string, string>): Record<string, string> {
  const masked: Record<string, string> = {}
  for (const [key, value] of Object.entries(headers)) {
    masked[key] = SECRET_PATTERNS.test(key) ? '****' : value
  }
  return masked
}
