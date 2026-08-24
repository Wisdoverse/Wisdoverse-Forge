import { describe, expect, test } from 'vitest'
import { summarizeOperations, type OperationsInput } from '@app/features/operations/model/triage'

function base(overrides: Partial<OperationsInput> = {}): OperationsInput {
  return {
    runtimeReady: true,
    providerVerified: true,
    providerCount: 1,
    availableAgents: 2,
    queueBacklog: 0,
    queueWorking: 1,
    queueCompleted: 3,
    healthChecks: { database: true, redis: true, nats: true, docker: true },
    healthStatus: 'ready',
    ...overrides,
  }
}

describe('operations triage', () => {
  test('all clear when every input is healthy', () => {
    const summary = summarizeOperations(base())
    expect(summary.allClear).toBe(true)
    expect(summary.attention).toEqual([])
  })

  test('lists each unmet prerequisite with its direct action', () => {
    const summary = summarizeOperations(
      base({ runtimeReady: false, providerVerified: false, providerCount: 0, availableAgents: 0 })
    )
    expect(summary.allClear).toBe(false)
    expect(summary.attention.map((item) => item.id)).toEqual(['runtime', 'providers', 'agents'])
    expect(summary.attention[0].path).toBe('/settings/runtime')
  })

  test('flags a stalled queue and degraded health with details', () => {
    const summary = summarizeOperations(
      base({
        queueBacklog: 4,
        queueWorking: 0,
        healthChecks: { database: true, redis: false, nats: true, docker: false },
        healthStatus: 'degraded',
      })
    )
    expect(summary.attention.map((item) => item.id)).toEqual(['queue', 'health'])
    expect(summary.healthDegradedDetails).toEqual(['redis', 'docker'])
  })
})
