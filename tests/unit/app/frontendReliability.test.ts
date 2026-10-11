import { afterEach, beforeEach, expect, test, vi } from 'vitest'

const user = { id: 'user-a', orgId: 'org-a' }
const base = 'https://staging.example.com/api/v1'
const token = (identity = user, version = 1): string =>
  `header.${btoa(JSON.stringify({ sub: identity.id, org: identity.orgId, version }))}.signature`

let reporter: typeof import('@app/shared/lib/frontendReliability')
let fetchMock: ReturnType<typeof vi.fn>
let listeners: Array<[string, EventListenerOrEventListenerObject]>

beforeEach(async () => {
  vi.resetModules()
  listeners = []
  const original = window.addEventListener.bind(window)
  vi.spyOn(window, 'addEventListener').mockImplementation((type, listener, options) => {
    if (listener) listeners.push([type, listener])
    original(type, listener, options)
  })
  fetchMock = vi.fn().mockResolvedValue({ ok: true })
  vi.stubGlobal('fetch', fetchMock)
  reporter = await import('@app/shared/lib/frontendReliability')
})

afterEach(() => {
  for (const [type, listener] of listeners) window.removeEventListener(type, listener)
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

function sent(index: number) {
  const [url, init] = fetchMock.mock.calls[index] as [string, RequestInit]
  return { url, init, body: JSON.parse(init.body as string) }
}

test('StrictMode observation and same-scope refresh retain one document session', () => {
  reporter.observeFrontendSession(user, token(), base)
  const start = sent(0)
  reporter.observeFrontendSession(user, token(), base)
  reporter.observeFrontendSession(user, token(user, 2), base)
  expect(fetchMock).toHaveBeenCalledTimes(1)
  expect(start.url).toBe(`${base}/analytics/events`)
  expect(start.body.properties.browserSessionId).toMatch(/^[a-f0-9-]{36}$/)
  reporter.recordFrontendCrash()
  expect(sent(1).body.properties.browserSessionId).toBe(start.body.properties.browserSessionId)
  expect(sent(1).init.headers).toMatchObject({ Authorization: `Bearer ${token(user, 2)}` })
  expect(sent(1).init.keepalive).toBe(true)
})

test('identity changes end the old scope with old credentials and allocate a new ID', () => {
  const other = { id: user.id, orgId: 'org-b' }
  reporter.observeFrontendSession(user, token(), base)
  reporter.observeFrontendSession(other, token(other), base)
  expect(fetchMock).toHaveBeenCalledTimes(3)
  expect(sent(1).body.event_name).toBe('frontend_session_ended')
  expect(sent(1).init.headers).toMatchObject({ Authorization: `Bearer ${token()}` })
  expect(sent(1).body.properties).toEqual(sent(0).body.properties)
  expect(sent(2).body.properties).not.toEqual(sent(0).body.properties)
  expect(sent(2).init.headers).toMatchObject({ Authorization: `Bearer ${token(other)}` })
  const nextUser = { id: 'user-b', orgId: other.orgId }
  reporter.observeFrontendSession(nextUser, token(nextUser), base)
  expect(sent(4).body.properties).not.toEqual(sent(2).body.properties)
})

test('offline and delayed 401 observations are never replayed under a new identity', async () => {
  let release!: (value: { ok: boolean; status: number }) => void
  fetchMock.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        release = resolve
      })
  )
  reporter.observeFrontendSession(user, token(), base)
  const other = { id: 'user-b', orgId: 'org-b' }
  fetchMock.mockRejectedValueOnce(new Error('offline private content'))
  reporter.observeFrontendSession(other, token(other), base)
  release({ ok: false, status: 401 })
  await Promise.resolve()
  await Promise.resolve()
  expect(fetchMock).toHaveBeenCalledTimes(3)
  expect(sent(0).init.headers).toMatchObject({ Authorization: `Bearer ${token()}` })
  expect(sent(1).init.headers).toMatchObject({ Authorization: `Bearer ${token()}` })
  expect(sent(2).init.headers).toMatchObject({ Authorization: `Bearer ${token(other)}` })
})

test('a lost crash remains failed locally and cannot be followed by a healthy end', async () => {
  reporter.observeFrontendSession(user, token(), base)
  fetchMock.mockRejectedValueOnce(new Error('private stack and token'))
  reporter.recordFrontendCrash()
  reporter.recordFrontendCrash()
  window.dispatchEvent(new PageTransitionEvent('pagehide', { persisted: false }))
  reporter.observeFrontendSession(null, null, base)
  await Promise.resolve()
  expect(fetchMock).toHaveBeenCalledTimes(2)
  expect(sent(1).body.event_name).toBe('frontend_session_crashed')
  expect(Object.keys(sent(1).body.properties)).toEqual(['browserSessionId'])
  expect(JSON.stringify(sent(1).body)).not.toMatch(/private|stack|token/)
})

test('visibility and BFCache transitions retain the session; a later real pagehide ends once', () => {
  reporter.observeFrontendSession(user, token(), base)
  document.dispatchEvent(new Event('visibilitychange'))
  window.dispatchEvent(new PageTransitionEvent('pagehide', { persisted: true }))
  window.dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true }))
  reporter.observeFrontendSession(user, token(), base)
  expect(fetchMock).toHaveBeenCalledTimes(1)
  window.dispatchEvent(new PageTransitionEvent('pagehide', { persisted: false }))
  window.dispatchEvent(new PageTransitionEvent('pagehide', { persisted: false }))
  expect(fetchMock).toHaveBeenCalledTimes(2)
  expect(sent(1).body.event_name).toBe('frontend_session_ended')
  // Errors after an observed end still override it at the server.
  reporter.recordFrontendCrash()
  expect(sent(2).body.event_name).toBe('frontend_session_crashed')
  expect(sent(2).body.properties).toEqual(sent(0).body.properties)
})

test.each(['error', 'unhandledrejection'])('global %s records one private observation', (type) => {
  reporter.observeFrontendSession(user, token(), base)
  window.dispatchEvent(new Event(type))
  window.dispatchEvent(new Event(type))
  expect(fetchMock).toHaveBeenCalledTimes(2)
  expect(sent(1).body.event_name).toBe('frontend_session_crashed')
  expect(Object.keys(sent(1).body.properties)).toEqual(['browserSessionId'])
})

test('missing or mismatched token scope cannot use a cached session with new credentials', () => {
  reporter.observeFrontendSession(null, null, base)
  reporter.observeFrontendSession(user, 'invalid', base)
  expect(fetchMock).not.toHaveBeenCalled()
  reporter.observeFrontendSession(user, token(), base)
  const other = { id: user.id, orgId: 'org-b' }
  reporter.observeFrontendSession(user, token(other), base)
  expect(fetchMock).toHaveBeenCalledTimes(2)
  expect(sent(1).body.event_name).toBe('frontend_session_ended')
  expect(sent(1).init.headers).toMatchObject({ Authorization: `Bearer ${token()}` })
  reporter.recordFrontendCrash()
  expect(fetchMock).toHaveBeenCalledTimes(2)
})

test('unavailable crypto or synchronously throwing transport cannot interrupt authentication or recovery', () => {
  vi.spyOn(crypto, 'randomUUID').mockImplementationOnce(() => {
    throw new Error('unavailable')
  })
  expect(() => reporter.observeFrontendSession(user, token(), base)).not.toThrow()
  fetchMock.mockImplementation(() => {
    throw new Error('fetch unavailable')
  })
  expect(() => reporter.observeFrontendSession(user, token(), base)).not.toThrow()
  expect(() => reporter.recordFrontendCrash()).not.toThrow()
  expect(() => reporter.observeFrontendSession(null, null, base)).not.toThrow()
})
