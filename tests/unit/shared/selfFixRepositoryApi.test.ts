import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  getSelfFixRepository,
  REPOSITORY_CHECK_TIMEOUT_MS,
  RepositorySetupError,
} from '@app/shared/api/selfFix'

const mocks = vi.hoisted(() => ({ fetch: vi.fn() }))
vi.mock('@app/shared/api/authFetch', () => ({ authFetch: mocks.fetch }))

const snapshot = {
  repository: 'example-org/example-repo',
  defaultBranch: 'release/stable',
  baseSha: 'a'.repeat(40),
  contentsWrite: true,
  pullRequestsWrite: true,
  checksRead: true,
  squashMergeAllowed: true,
}

function jsonResponse(payload: unknown, status = 200): Response {
  return new Response(JSON.stringify(payload), { status })
}

beforeEach(() => mocks.fetch.mockResolvedValue(jsonResponse({ ok: true, data: snapshot })))
afterEach(() => vi.useRealTimers())

describe('repository setup API boundary', () => {
  it('uses the authenticated, uncached GET contract and strips fields outside that contract', async () => {
    mocks.fetch.mockResolvedValueOnce(
      jsonResponse({
        ok: true,
        data: { ...snapshot, installationToken: 'test-installation-token' },
      })
    )
    expect(await getSelfFixRepository()).toEqual(snapshot)
    expect(mocks.fetch).toHaveBeenCalledWith('/api/v1/self-fix/repository', {
      method: 'GET',
      cache: 'no-store',
      headers: { Accept: 'application/json' },
      signal: expect.any(AbortSignal),
    })
  })

  it.each([
    { ok: false, data: snapshot },
    { ok: true, data: { ...snapshot, repository: 'example-org/example-repo/extra' } },
    { ok: true, data: { ...snapshot, baseSha: '' } },
    { ok: true, data: { ...snapshot, defaultBranch: ' ' } },
    { ok: true, data: { ...snapshot, checksRead: 'true' } },
    { ok: true, data: { ...snapshot, contentsWrite: null } },
    { ok: true, data: { ...snapshot, pullRequestsWrite: 1 } },
    { ok: true, data: { ...snapshot, squashMergeAllowed: undefined } },
  ])('fails closed on malformed setup data (%#)', async (payload) => {
    mocks.fetch.mockResolvedValueOnce(jsonResponse(payload))
    await expect(getSelfFixRepository()).rejects.toMatchObject({ reason: 'invalid-response' })
  })

  it('rejects a successful HTTP response containing non-JSON content', async () => {
    mocks.fetch.mockResolvedValueOnce(new Response('unexpected response'))
    await expect(getSelfFixRepository()).rejects.toMatchObject({ reason: 'invalid-response' })
  })

  it.each([
    [401, 'unauthenticated'],
    [403, 'forbidden'],
    [404, 'unsupported'],
    [500, 'failed'],
  ])('classifies HTTP %s without retaining the upstream body', async (status, reason) => {
    mocks.fetch.mockResolvedValueOnce(
      jsonResponse(
        {
          error: { message: 'private.example.invalid test-installation-token' },
        },
        status as number
      )
    )
    const error = await getSelfFixRepository().catch((value: unknown) => value)
    expect(error).toBeInstanceOf(RepositorySetupError)
    expect(error).toMatchObject({ reason })
    expect(String(error)).not.toContain('private.example.invalid')
    expect(String(error)).not.toContain('test-installation-token')
  })

  it.each([
    ['github_not_configured', 'not-configured'],
    ['repository_permissions', 'permissions'],
    ['repository_unavailable', 'unavailable'],
    ['repository_access', 'access'],
    ['repository_base_unavailable', 'empty-repository'],
    ['unknown_future_error', 'failed'],
  ])('maps a typed configuration failure %s to safe guidance', async (code, reason) => {
    mocks.fetch.mockResolvedValueOnce(
      jsonResponse(
        {
          ok: false,
          error: { code: `errors.self_fix.${code}`, message: 'test-installation-token' },
        },
        400
      )
    )
    await expect(getSelfFixRepository()).rejects.toMatchObject({ reason })
  })

  it('sanitizes transport errors', async () => {
    mocks.fetch.mockRejectedValueOnce(new Error('private.example.invalid test-installation-token'))
    const error = await getSelfFixRepository().catch((value: unknown) => value)
    expect(error).toMatchObject({ reason: 'unreachable' })
    expect(String(error)).not.toContain('test-installation-token')
  })

  it('does not send an already-cancelled request', async () => {
    const controller = new AbortController()
    controller.abort()
    await expect(getSelfFixRepository(controller.signal)).rejects.toMatchObject({
      reason: 'cancelled',
    })
    expect(mocks.fetch).not.toHaveBeenCalled()
  })

  it('cancels immediately even when the transport ignores the abort signal', async () => {
    mocks.fetch.mockReturnValueOnce(new Promise(() => undefined))
    const controller = new AbortController()
    const result = expect(getSelfFixRepository(controller.signal)).rejects.toMatchObject({
      reason: 'cancelled',
    })
    controller.abort()
    await result
    expect(mocks.fetch.mock.calls[0][1].signal.aborted).toBe(true)
  })

  it('bounds the entire check, including reading the response body', async () => {
    vi.useFakeTimers()
    mocks.fetch.mockResolvedValueOnce(new Response(new ReadableStream()))
    const result = expect(getSelfFixRepository()).rejects.toMatchObject({ reason: 'timeout' })
    await vi.advanceTimersByTimeAsync(REPOSITORY_CHECK_TIMEOUT_MS)
    await result
    expect(mocks.fetch.mock.calls[0][1].signal.aborted).toBe(true)
    expect(vi.getTimerCount()).toBe(0)
  })
})
