export interface RadarOptions {
  radarUrl: string
  consumerId: string
  serviceId: string
  token?: string
  flushIntervalMs?: number
  maxBatch?: number
}

/**
 * Structurally compatible with express's RequestHandler, without requiring
 * `@types/express`: express is an optional peer dependency, so these types
 * must resolve for consumers that don't install it.
 */
export type RadarRequestHandler = (req: any, res: any, next: (err?: unknown) => void) => void

export class RadarBatcher {
  constructor(opts: RadarOptions)
  push(operation: string, fieldPath?: string): void
  flush(): void
  destroy(): void
}

export function expressMiddleware(opts: RadarOptions): RadarRequestHandler
export function recordFieldUsage(batcher: RadarBatcher, operation: string, fieldPath: string): void
