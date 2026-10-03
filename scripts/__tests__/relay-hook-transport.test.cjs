'use strict'

const assert = require('node:assert/strict')
const crypto = require('node:crypto')
const { EventEmitter } = require('node:events')
const fs = require('node:fs')
const path = require('node:path')
const test = require('node:test')
const vm = require('node:vm')

const hookPath = path.resolve(__dirname, '../../hooks/agentforge-relay-hook.cjs')
const source = fs.readFileSync(hookPath, 'utf8')
const beforeMain = source.slice(0, source.indexOf('async function main()'))
const agentId = '2f1f4e2d-52ba-4b88-9221-e323ef2eb091'
const hmacSecret = 'synthetic-hook-hmac-secret'
const eventJson = JSON.stringify({ type: 'post_tool_use', text: 'synthetic-hook-event' })

class FakeSocket extends EventEmitter {
  writes = []
  timeoutMs = undefined
  idleTimeout = undefined
  ended = false
  destroyed = false

  setTimeout(ms, callback) {
    this.timeoutMs = ms
    this.idleTimeout = callback
    return this
  }

  write(value, callback) {
    this.writes.push(Buffer.from(value))
    if (callback) queueMicrotask(() => callback())
    return true
  }

  end() {
    this.ended = true
    this.emit('close')
  }

  destroy() {
    this.destroyed = true
    this.emit('close')
  }
}

function createFakeTimers() {
  const timers = []
  return {
    timers,
    setTimeout(callback, ms) {
      const timer = { callback, ms, cleared: false, fired: false }
      timers.push(timer)
      return timer
    },
    clearTimeout(timer) {
      if (timer) timer.cleared = true
    },
    fireNext() {
      const timer = timers.find((candidate) => !candidate.cleared && !candidate.fired)
      assert.ok(timer, 'expected an active absolute timer')
      timer.fired = true
      timer.callback()
    },
  }
}

function loadHook(platform, env = {}, timers = createFakeTimers()) {
  const sockets = []
  const endpoints = []
  const fakeNet = {
    createConnection(endpoint, onConnect) {
      endpoints.push(endpoint)
      const socket = new FakeSocket()
      sockets.push(socket)
      queueMicrotask(() => {
        if (onConnect) onConnect()
        else socket.emit('connect')
      })
      return socket
    },
  }
  const module = { exports: {} }
  const sandbox = {
    Buffer,
    module,
    exports: module.exports,
    process: { platform, env: { ...env } },
    setTimeout: timers.setTimeout,
    clearTimeout: timers.clearTimeout,
    require(id) {
      return id === 'node:net' ? fakeNet : require(id)
    },
  }

  vm.runInNewContext(`${beforeMain}\nmodule.exports = { sendEvent };`, sandbox, {
    filename: hookPath,
  })
  return { sendEvent: module.exports.sendEvent, sockets, endpoints, timers }
}

async function until(predicate) {
  for (let attempt = 0; attempt < 50; attempt += 1) {
    if (predicate()) return
    await new Promise(setImmediate)
  }
  assert.fail('timed out waiting for fake socket activity')
}

function serverProof(nonce, secret = hmacSecret) {
  return crypto
    .createHmac('sha256', Buffer.from(secret, 'utf8'))
    .update(Buffer.concat([Buffer.from('agentforge-relay-server-v1\0', 'utf8'), nonce]))
    .digest()
}

test('Windows authenticates the named-pipe server before sending the unchanged frame', async () => {
  const hook = loadHook('win32', { AGENTFORGE_AGENT_ID: agentId, HMAC_SECRET: hmacSecret })
  const pending = hook.sendEvent(eventJson)
  await until(() => hook.sockets[0]?.writes.length === 1)

  assert.deepEqual(hook.endpoints, [`\\\\.\\pipe\\agentforge-relay-${agentId}`])
  const socket = hook.sockets[0]
  assert.equal(socket.timeoutMs, 2000)
  assert.equal(socket.writes[0].length, 32)
  assert.equal(socket.writes.length, 1, 'event must wait for a valid server proof')
  assert.equal(hook.timers.timers[0].ms, 2000)

  const proof = serverProof(socket.writes[0])
  socket.emit('data', proof.subarray(0, 11))
  await new Promise(setImmediate)
  assert.equal(socket.writes.length, 1, 'partial proof must not release the event')
  socket.emit('data', proof.subarray(11))
  assert.equal(socket.listenerCount('data'), 0, 'proof listener is removed after authentication')
  await pending

  assert.equal(socket.writes.length, 2)
  const frame = socket.writes[1]
  assert.equal(frame.readUInt32BE(0), Buffer.byteLength(eventJson))
  assert.equal(frame.subarray(4).toString('utf8'), eventJson)
  assert.equal(socket.ended, true)
  assert.equal(hook.timers.timers[0].cleared, true)
})

test('Windows rejects missing or non-canonical identity and missing HMAC without UDS fallback', async () => {
  const cases = [
    [{ HMAC_SECRET: hmacSecret }, /AGENTFORGE_AGENT_ID/],
    [
      { AGENTFORGE_AGENT_ID: '2F1F4E2D-52BA-4B88-9221-E323EF2EB091', HMAC_SECRET: hmacSecret },
      /AGENTFORGE_AGENT_ID/,
    ],
    [{ AGENTFORGE_AGENT_ID: agentId }, /HMAC_SECRET/],
    [{ AGENTFORGE_AGENT_ID: agentId, HMAC_SECRET: '' }, /HMAC_SECRET/],
  ]

  for (const [env, message] of cases) {
    const hook = loadHook('win32', env)
    await assert.rejects(hook.sendEvent(eventJson), message)
    assert.deepEqual(hook.endpoints, [])
  }
})

test('Windows rejects stale, wrong-key, invalid, oversized, or partial-EOF proof without sending the event', async () => {
  for (const proofKind of ['invalid', 'stale', 'wrong-key', 'oversized', 'partial-eof']) {
    const hook = loadHook('win32', { AGENTFORGE_AGENT_ID: agentId, HMAC_SECRET: hmacSecret })
    const pending = hook.sendEvent(eventJson)
    await until(() => hook.sockets[0]?.writes.length === 1)
    const socket = hook.sockets[0]

    if (proofKind === 'invalid') socket.emit('data', Buffer.alloc(32))
    else if (proofKind === 'stale') socket.emit('data', serverProof(Buffer.alloc(32, 0x5a)))
    else if (proofKind === 'wrong-key')
      socket.emit('data', serverProof(socket.writes[0], 'wrong synthetic key'))
    else if (proofKind === 'oversized') socket.emit('data', Buffer.alloc(33))
    else {
      socket.emit('data', Buffer.alloc(11))
      socket.emit('end')
    }

    await assert.rejects(pending, /Windows relay/)
    assert.equal(socket.writes.length, 1, `${proofKind} proof must not release the event`)
  }
})

test('Windows absolute deadline rejects a trickled proof without sending the event', async () => {
  const hook = loadHook('win32', { AGENTFORGE_AGENT_ID: agentId, HMAC_SECRET: hmacSecret })
  const pending = hook.sendEvent(eventJson)
  await until(() => hook.sockets[0]?.writes.length === 1)
  const socket = hook.sockets[0]

  socket.emit('data', Buffer.alloc(11))
  hook.timers.fireNext()
  await assert.rejects(pending, /proof timeout/)
  assert.equal(socket.writes.length, 1, 'timeout must leave only the nonce on the pipe')
  assert.equal(hook.timers.timers[0].cleared, true)
})

test('Unix keeps the fixed UDS endpoint and four-byte big-endian JSON frame', async () => {
  const hook = loadHook('linux')
  await hook.sendEvent(eventJson)

  assert.deepEqual(hook.endpoints, ['/tmp/agentforge-relay.sock'])
  const writes = hook.sockets[0].writes
  assert.equal(writes.length, 2)
  assert.equal(writes[0].readUInt32BE(0), Buffer.byteLength(eventJson))
  assert.equal(writes[1].toString('utf8'), eventJson)
})
