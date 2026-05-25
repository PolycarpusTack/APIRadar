'use strict'

const http = require('http')
const https = require('https')
const { URL } = require('url')

// ---------------------------------------------------------------------------
// Internal batch queue
// ---------------------------------------------------------------------------

const DEFAULT_FLUSH_INTERVAL_MS = 5000
const DEFAULT_MAX_BATCH = 100

class RadarBatcher {
  constructor ({ radarUrl, consumerId, serviceId, token, flushIntervalMs, maxBatch }) {
    this._url = new URL('/v1/usage/events', radarUrl)
    this._consumerId = consumerId
    this._serviceId = serviceId
    this._token = token
    this._maxBatch = maxBatch || DEFAULT_MAX_BATCH
    this._queue = []
    this._timer = setInterval(() => this.flush(), flushIntervalMs || DEFAULT_FLUSH_INTERVAL_MS)
    this._timer.unref() // don't block process exit
  }

  push (operation, fieldPath) {
    if (this._queue.length >= this._maxBatch * 2) return // back-pressure: drop silently
    this._queue.push({
      consumer_id: this._consumerId,
      service_id: this._serviceId,
      operation,
      field_path: fieldPath || ''
    })
    if (this._queue.length >= this._maxBatch) this.flush()
  }

  flush () {
    if (this._queue.length === 0) return
    const batch = this._queue.splice(0, this._maxBatch)
    const body = JSON.stringify(batch)
    const options = {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Content-Length': Buffer.byteLength(body),
        ...(this._token ? { Authorization: `Bearer ${this._token}` } : {})
      }
    }
    const lib = this._url.protocol === 'https:' ? https : http
    const req = lib.request(this._url, options, () => {})
    req.on('error', () => {}) // fire-and-forget; don't crash on network errors
    req.write(body)
    req.end()
  }

  destroy () {
    clearInterval(this._timer)
    this.flush()
  }
}

// ---------------------------------------------------------------------------
// Express middleware
// ---------------------------------------------------------------------------

/**
 * Create an Express middleware that reports API usage events to Radar Monitor.
 *
 * @param {object} opts
 * @param {string} opts.radarUrl         - Base URL of the radar-api server (e.g. 'http://localhost:8080')
 * @param {string} opts.consumerId       - ID of this consumer service in Radar
 * @param {string} opts.serviceId        - ID of the upstream producer service being called
 * @param {string} [opts.token]          - Optional bearer token
 * @param {number} [opts.flushIntervalMs] - How often to flush the batch (default: 5000)
 * @param {number} [opts.maxBatch]       - Max events per flush (default: 100)
 * @returns {function} Express middleware
 */
function expressMiddleware (opts) {
  const batcher = new RadarBatcher(opts)

  return function radarMonitor (req, res, next) {
    res.on('finish', () => {
      const method = req.method || 'GET'
      // Prefer the matched route pattern over the raw URL to avoid per-ID noise.
      const route = (req.route && req.route.path)
        ? req.route.path
        : req.path || req.url
      const operation = `${method.toUpperCase()} ${route}`
      batcher.push(operation, '')
    })
    next()
  }
}

/**
 * Manually record a field-level usage event (e.g. from a response parser).
 *
 * @param {RadarBatcher} batcher
 * @param {string} operation - e.g. 'GET /users/{id}'
 * @param {string} fieldPath - dot-separated field path e.g. 'user.email'
 */
function recordFieldUsage (batcher, operation, fieldPath) {
  batcher.push(operation, fieldPath)
}

module.exports = { expressMiddleware, RadarBatcher, recordFieldUsage }
