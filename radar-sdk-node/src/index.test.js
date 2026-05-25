'use strict'

const { test } = require('node:test')
const assert = require('node:assert/strict')
const { RadarBatcher, expressMiddleware, recordFieldUsage } = require('./index.js')

test('RadarBatcher queues events up to maxBatch', () => {
  const batcher = new RadarBatcher({
    radarUrl: 'http://localhost:8080',
    consumerId: 'c1',
    serviceId: 's1',
    maxBatch: 3,
    flushIntervalMs: 999999,
  })
  batcher.push('GET /users', '')
  batcher.push('POST /orders', '')
  assert.equal(batcher._queue.length, 2)
  batcher.destroy()
})

test('RadarBatcher auto-flushes when maxBatch reached', () => {
  // With maxBatch=1, the second push triggers a flush (queue goes to 0).
  const batcher = new RadarBatcher({
    radarUrl: 'http://localhost:8080',
    consumerId: 'c1',
    serviceId: 's1',
    maxBatch: 1,
    flushIntervalMs: 999999,
  })
  batcher.push('GET /a', '')
  // Flush clears the queue but makes a network call (which fails silently to localhost:8080).
  batcher.push('GET /b', '')
  // After the first push hit maxBatch, queue was flushed; second push fills queue again.
  assert.ok(batcher._queue.length <= 1)
  batcher.destroy()
})

test('recordFieldUsage delegates to batcher.push', () => {
  let pushed = null
  const fakeBatcher = { push: (op, fp) => { pushed = { op, fp } } }
  recordFieldUsage(fakeBatcher, 'GET /items', 'item.price')
  assert.deepEqual(pushed, { op: 'GET /items', fp: 'item.price' })
})

test('expressMiddleware returns a function with 3 arguments', () => {
  const mw = expressMiddleware({
    radarUrl: 'http://localhost:8080',
    consumerId: 'c1',
    serviceId: 's1',
    flushIntervalMs: 999999,
  })
  assert.equal(typeof mw, 'function')
  assert.equal(mw.length, 3)
})
