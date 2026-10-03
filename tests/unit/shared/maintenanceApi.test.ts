import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  getMaintenanceTrace,
  submitMaintenanceRequest,
  MAINTENANCE_TIMEOUT_MS,
} from '@app/entities/maintenance'
import {
  maintenanceTaskId,
  maintenanceRequestId,
  maintenanceGroupId,
  maintenanceTrace,
} from '../../support/fixtures/maintenance'

const mocks = vi.hoisted(() => ({ fetch: vi.fn() }))
vi.mock('@app/shared/api/authFetch', () => ({ authFetch: mocks.fetch }))
const input = {
  groupId: maintenanceGroupId,
  title: 'Fix dependencies',
  brief: 'Run the dependency checks',
  source: { kind: 'request' as const, reference: 'dependency-2026-10' },
}
const submission = { requestId: maintenanceRequestId, taskId: maintenanceTaskId, reused: false }
const response = (data: unknown) => new Response(JSON.stringify({ ok: true, data }))
beforeEach(() => mocks.fetch.mockResolvedValue(response(maintenanceTrace())))
afterEach(() => vi.useRealTimers())

describe('maintenance API boundary', () => {
  it('sends explicit intake through authenticated fetch and strips undeclared fields', async () => {
    mocks.fetch.mockResolvedValueOnce(
      response({ ...submission, installationToken: 'synthetic-secret' })
    )
    expect(await submitMaintenanceRequest(input)).toEqual(submission)
    expect(mocks.fetch).toHaveBeenCalledWith(
      '/api/v1/self-fix/requests',
      expect.objectContaining({
        method: 'POST',
        cache: 'no-store',
        body: JSON.stringify(input),
        signal: expect.any(AbortSignal),
      })
    )
  })
  it('reads a trace for the exact requested task and strips undeclared fields recursively', async () => {
    const trace = maintenanceTrace()
    mocks.fetch.mockResolvedValueOnce(
      response({
        ...trace,
        secret: 'synthetic-secret',
        source: { ...trace.source, token: 'synthetic-secret' },
      })
    )
    expect(await getMaintenanceTrace(maintenanceTaskId)).toEqual(trace)
  })
  it('allows older tasks without a source record', async () => {
    mocks.fetch.mockResolvedValueOnce(response(null))
    expect(await getMaintenanceTrace(maintenanceTaskId)).toBeNull()
  })
  it('canonicalizes GitHub repository casing while retaining only the approved repository path', async () => {
    const trace = maintenanceTrace()
    trace.producedPrUrl = 'https://github.com/example-org/example-repo/pull/42'
    mocks.fetch.mockResolvedValueOnce(
      response({ ...trace, producedPrUrl: 'https://github.com/Example-Org/Example-Repo/pull/42' })
    )
    expect(await getMaintenanceTrace(maintenanceTaskId)).toEqual(trace)
  })
  it.each([
    { taskId: maintenanceRequestId },
    { startingSha: 'invalid' },
    { createdAt: 'invalid' },
    { repository: 'example-org/example-repo/extra' },
    { attempt: -1 },
    { executions: [{}] },
    { producedPrUrl: 'javascript:alert(1)' },
    { producedPrUrl: 'https://github.com/other/repo/pull/42' },
    {
      source: {
        kind: 'request',
        reference: 'x',
        url: 'https://github.com/example-org/example-repo/pull/42',
        submittedHeadSha: null,
      },
    },
    {
      producedState: {
        status: 'observed',
        checkedAt: '2026-10-01T12:00:00Z',
        snapshot: null,
        headChanged: false,
      },
    },
    {
      producedState: { status: 'unavailable', checkedAt: null, snapshot: null, headChanged: true },
    },
  ])('rejects incompatible trace or unsafe links (%#)', async (patch) => {
    mocks.fetch.mockResolvedValueOnce(response({ ...maintenanceTrace(), ...patch }))
    await expect(getMaintenanceTrace(maintenanceTaskId)).rejects.toMatchObject({
      reason: 'invalid-response',
    })
  })
  it('accepts a current changed PR head separately from recorded history', async () => {
    const trace = maintenanceTrace()
    trace.recordedPrHeadSha = 'b'.repeat(40)
    trace.producedPrUrl = 'https://github.com/example-org/example-repo/pull/42'
    trace.producedState = {
      status: 'observed',
      checkedAt: '2026-10-01T12:05:00Z',
      headChanged: true,
      snapshot: {
        number: 42,
        url: trace.producedPrUrl,
        headSha: 'c'.repeat(40),
        baseBranch: trace.defaultBranch,
        state: 'open',
      },
    }
    expect(await getMaintenanceTrace(maintenanceTaskId)).toEqual(maintenanceTrace())
    mocks.fetch.mockResolvedValueOnce(response(trace))
    expect(await getMaintenanceTrace(maintenanceTaskId)).toEqual(trace)
  })
  it.each([
    [401, 'unauthenticated'],
    [403, 'forbidden'],
    [404, 'missing'],
    [500, 'failed'],
  ])('sanitizes HTTP %s', async (status, reason) => {
    mocks.fetch.mockResolvedValueOnce(
      new Response('synthetic-secret', { status: status as number })
    )
    const error = await submitMaintenanceRequest(input).catch((e: unknown) => e)
    expect(error).toMatchObject({ reason })
    expect(String(error)).not.toContain('synthetic-secret')
  })
  it.each([
    ['destination_unavailable', 'destination'],
    ['source_closed', 'source-closed'],
    ['source_base', 'source-base'],
    ['source_unavailable', 'source-unavailable'],
  ])('maps %s to bounded recovery guidance', async (code, reason) => {
    mocks.fetch.mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          error: { code: `errors.maintenance.${code}`, message: 'synthetic-secret' },
        }),
        { status: 400 }
      )
    )
    await expect(submitMaintenanceRequest(input)).rejects.toMatchObject({ reason })
  })
  it('bounds body reads for an uncertain submission without automatically resubmitting', async () => {
    vi.useFakeTimers()
    mocks.fetch.mockResolvedValueOnce(new Response(new ReadableStream()))
    const pending = expect(submitMaintenanceRequest(input)).rejects.toMatchObject({
      reason: 'timeout',
    })
    await vi.advanceTimersByTimeAsync(MAINTENANCE_TIMEOUT_MS)
    await pending
    expect(mocks.fetch).toHaveBeenCalledTimes(1)
    expect(mocks.fetch.mock.calls[0][1].signal.aborted).toBe(true)
    expect(vi.getTimerCount()).toBe(0)
  })
  it('cancels immediately if the transport ignores abort', async () => {
    mocks.fetch.mockReturnValueOnce(new Promise(() => undefined))
    const controller = new AbortController()
    const pending = expect(
      getMaintenanceTrace(maintenanceTaskId, controller.signal)
    ).rejects.toMatchObject({ reason: 'cancelled' })
    controller.abort()
    await pending
  })
  it('does not send an already cancelled request or invalid task path', async () => {
    const controller = new AbortController()
    controller.abort()
    await expect(submitMaintenanceRequest(input, controller.signal)).rejects.toMatchObject({
      reason: 'cancelled',
    })
    await expect(getMaintenanceTrace('../admin')).rejects.toMatchObject({
      reason: 'invalid-request',
    })
    expect(mocks.fetch).not.toHaveBeenCalled()
  })
})
