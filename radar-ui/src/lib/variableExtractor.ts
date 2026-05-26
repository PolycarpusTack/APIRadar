const VAR_REGEX = /\{\{([a-zA-Z_][a-zA-Z0-9_]*)\}\}/g

export interface PlaygroundRequest {
  url: string
  method: string
  headers: { key: string; value: string }[]
  body: string
}

/** Extract all unique {{variable_name}} placeholders from a request template. */
export function extractVariables(request: PlaygroundRequest): string[] {
  const found = new Set<string>()

  const scan = (text: string) => {
    let match: RegExpExecArray | null
    VAR_REGEX.lastIndex = 0
    while ((match = VAR_REGEX.exec(text)) !== null) {
      found.add(match[1])
    }
  }

  scan(request.url)
  scan(request.body)
  for (const h of request.headers) {
    scan(h.key)
    scan(h.value)
  }

  return Array.from(found)
}
