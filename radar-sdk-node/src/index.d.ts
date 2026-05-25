import { RequestHandler } from 'express'

export interface RadarOptions {
  radarUrl: string
  consumerId: string
  serviceId: string
  token?: string
  flushIntervalMs?: number
  maxBatch?: number
}

export class RadarBatcher {
  constructor(opts: RadarOptions): void
  push(operation: string, fieldPath?: string): void
  flush(): void
  destroy(): void
}

export function expressMiddleware(opts: RadarOptions): RequestHandler
export function recordFieldUsage(batcher: RadarBatcher, operation: string, fieldPath: string): void
