import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  compareMaintenanceReports,
  createVerification,
  getMaintenanceDelivery,
  getMaintenanceOutcomes,
  MAINTENANCE_TIMEOUT_MS,
  recordMaintenanceDecision,
} from '@app/entities/maintenance'
import { maintenanceTaskId } from '../../support/fixtures/maintenance'
import {
  maintenanceDelivery,
  maintenanceDeliveryComparison,
  maintenanceDeliveryDecision,
  maintenanceDeliveryOutcomes,
} from '../../support/fixtures/maintenance-delivery'

const mocks = vi.hoisted(() => ({ fetch: vi.fn() }))
vi.mock('@app/shared/api/authFetch', () => ({ authFetch: mocks.fetch }))
const response = (data: unknown) => new Response(JSON.stringify({ ok: true, data }))
beforeEach(() => mocks.fetch.mockResolvedValue(response(maintenanceDelivery())))
afterEach(() => vi.useRealTimers())

describe('revision-bound maintenance evidence boundary', () => {
  it('retains reported checks, observed checks and human verdicts separately and strips extra fields', async () => {
    const data = maintenanceDelivery()
    mocks.fetch.mockResolvedValue(
      response({
        ...data,
        token: 'synthetic-secret',
        reports: data.reports.map((r) => ({
          ...r,
          snapshot: {
            ...r.snapshot,
            downloadUrl: 'https://example.com/private',
            github: { ...r.snapshot.github, authorization: 'synthetic-secret' },
          },
        })),
      })
    )
    expect(await getMaintenanceDelivery(maintenanceTaskId)).toEqual(data)
  })
  it('rejects task or repository substitution before displaying another report', async () => {
    const data = maintenanceDelivery()
    data.reports[0].taskId = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa'
    mocks.fetch.mockResolvedValueOnce(response(data))
    await expect(getMaintenanceDelivery(maintenanceTaskId)).rejects.toMatchObject({
      reason: 'invalid-response',
    })
    data.reports[0].taskId = maintenanceTaskId
    data.reports[0].snapshot.repository = 'other-org/other-repo'
    mocks.fetch.mockResolvedValueOnce(response(data))
    await expect(getMaintenanceDelivery(maintenanceTaskId)).rejects.toMatchObject({
      reason: 'invalid-response',
    })
  })
  it('rejects unbound execution evidence, unsafe revisions and fabricated totals', async () => {
    const samples = [maintenanceDelivery(), maintenanceDelivery(), maintenanceDelivery()]
    samples[0].reports[0].snapshot.runtime!.runId = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa'
    samples[1].currentRevision = '../../other'
    samples[2].decisions[0].humanMinutes.setup = null
    for (const sample of samples) {
      mocks.fetch.mockResolvedValueOnce(response(sample))
      await expect(getMaintenanceDelivery(maintenanceTaskId)).rejects.toMatchObject({
        reason: 'invalid-response',
      })
    }
  })
  it('keeps an incomplete effort total unknown while preserving an explicit zero', async () => {
    const data = maintenanceDelivery()
    data.decisions[0].humanMinutes.setup = null
    data.decisions[0].totalMinutes = null
    mocks.fetch.mockResolvedValueOnce(response(data))
    expect((await getMaintenanceDelivery(maintenanceTaskId))?.decisions[0].totalMinutes).toBeNull()
    Object.keys(data.decisions[0].humanMinutes).forEach((key) => {
      data.decisions[0].humanMinutes[
        key as keyof import('@shared/types/maintenance-delivery').HumanMinutes
      ] = 0
    })
    data.decisions[0].totalMinutes = 0
    mocks.fetch.mockResolvedValueOnce(response(data))
    expect((await getMaintenanceDelivery(maintenanceTaskId))?.decisions[0].totalMinutes).toBe(0)
  })
  it('allows ordinary self-fix tasks without maintenance records', async () => {
    mocks.fetch.mockResolvedValueOnce(response(null))
    expect(await getMaintenanceDelivery(maintenanceTaskId)).toBeNull()
  })
  it('fails closed on unknown provider observation statuses and contradictory verdict cohorts', async () => {
    mocks.fetch.mockResolvedValueOnce(
      response({ ...maintenanceDelivery(), currentChecks: { status: 'green' } })
    )
    await expect(getMaintenanceDelivery(maintenanceTaskId)).rejects.toMatchObject({
      reason: 'invalid-response',
    })
    const data = maintenanceDeliveryOutcomes()
    data.summary.awaitingReview = 1
    mocks.fetch.mockResolvedValueOnce(response(data))
    await expect(getMaintenanceOutcomes()).rejects.toMatchObject({ reason: 'invalid-response' })
  })
  it('passes a fixed submission reference and exact version to authenticated writes', async () => {
    const data = maintenanceDelivery(),
      input = {
        requestKey: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa',
        expectedVersion: data.taskVersion,
        expectedRevision: data.currentRevision,
        runId: data.latestRunId,
        criteria: 'tests pass',
        scope: 'one module',
        environmentNotes: 'same fixture',
        changeSummary: 'changed module',
        checks: [],
        unverified: [],
      }
    mocks.fetch.mockResolvedValueOnce(response(data.reports[0]))
    await createVerification(maintenanceTaskId, input)
    expect(mocks.fetch).toHaveBeenCalledWith(
      `/api/v1/self-fix/tasks/${maintenanceTaskId}/reports`,
      expect.objectContaining({
        body: JSON.stringify(input),
        cache: 'no-store',
        signal: expect.any(AbortSignal),
      })
    )
  })
  it('maps stale submissions and unavailable acceptance without exposing provider error text', async () => {
    const input = {
      requestKey: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa',
      reportId: maintenanceDeliveryDecision().reportId,
      expectedVersion: 3,
      expectedRevision: 'b'.repeat(40),
      verdict: 'accepted' as const,
      reason: 'reviewed',
      humanMinutes: {},
    }
    mocks.fetch.mockResolvedValueOnce(new Response('private provider details', { status: 409 }))
    await expect(recordMaintenanceDecision(maintenanceTaskId, input)).rejects.toMatchObject({
      reason: 'conflict',
    })
    mocks.fetch.mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          error: {
            code: 'errors.maintenance.delivery_unavailable',
            message: 'private provider details',
          },
        }),
        { status: 400 }
      )
    )
    await expect(recordMaintenanceDecision(maintenanceTaskId, input)).rejects.toMatchObject({
      reason: 'delivery-unavailable',
    })
  })
  it('bounds slow response bodies and cancels scoped reads', async () => {
    vi.useFakeTimers()
    mocks.fetch.mockResolvedValueOnce({ ok: true, status: 200, json: () => new Promise(() => {}) })
    const pending = getMaintenanceDelivery(maintenanceTaskId)
    const assertion = expect(pending).rejects.toMatchObject({ reason: 'timeout' })
    await vi.advanceTimersByTimeAsync(MAINTENANCE_TIMEOUT_MS)
    await assertion
    mocks.fetch.mockImplementationOnce(
      (_path, init) =>
        new Promise((_resolve, reject) =>
          init.signal.addEventListener('abort', () => reject(new Error('aborted')))
        )
    )
    const controller = new AbortController(),
      cancelled = getMaintenanceDelivery(maintenanceTaskId, controller.signal)
    controller.abort()
    await expect(cancelled).rejects.toMatchObject({ reason: 'cancelled' })
  })
  it('requires comparison responses to return exactly the requested report identities', async () => {
    const data = maintenanceDeliveryComparison(),
      ids = data.rows.map((row) => row.report.id)
    mocks.fetch.mockResolvedValueOnce(response(data))
    expect(await compareMaintenanceReports(ids)).toEqual(data)
    data.rows[1].report.id = 'cccccccc-cccc-4ccc-8ccc-cccccccccccc'
    mocks.fetch.mockResolvedValueOnce(response(data))
    await expect(compareMaintenanceReports(ids)).rejects.toMatchObject({
      reason: 'invalid-response',
    })
  })
})
