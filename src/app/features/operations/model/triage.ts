// Pure triage summaries for the Operations overview. Kept in the feature
// model so the rules are unit-testable without React.

export interface OperationsInput {
  runtimeReady: boolean
  providerVerified: boolean
  providerCount: number
  availableAgents: number
  queueBacklog: number
  queueWorking: number
  queueCompleted: number
  healthChecks?: Record<string, boolean>
  healthStatus?: string
}

export interface OperationsAttention {
  id: string
  /** i18n key prefix under operations.cards.<id> */
  statusKey: string
  path: string
}

export interface OperationsSummary {
  allClear: boolean
  attention: OperationsAttention[]
  healthDegradedDetails: string[]
}

export function summarizeOperations(input: OperationsInput): OperationsSummary {
  const attention: OperationsAttention[] = []
  if (!input.runtimeReady) {
    attention.push({
      id: 'runtime',
      statusKey: 'operations.cards.runtime.notReady',
      path: '/settings/runtime',
    })
  }
  if (!input.providerVerified) {
    attention.push({
      id: 'providers',
      statusKey:
        input.providerCount > 0
          ? 'operations.cards.providers.needsTest'
          : 'operations.cards.providers.none',
      path: '/settings/providers',
    })
  }
  if (input.availableAgents === 0) {
    attention.push({ id: 'agents', statusKey: 'operations.cards.agents.none', path: '/agents' })
  }
  if (input.queueBacklog > 0 && input.queueWorking === 0) {
    attention.push({ id: 'queue', statusKey: 'operations.cards.queue.stalled', path: '/tasks' })
  }

  const healthDegradedDetails: string[] = input.healthChecks
    ? Object.entries(input.healthChecks)
        .filter(([, ok]) => ok === false)
        .map(([name]) => name)
    : []
  if (input.healthStatus && input.healthStatus !== 'ready') {
    if (healthDegradedDetails.length === 0) healthDegradedDetails.push(input.healthStatus)
  }
  if (healthDegradedDetails.length > 0) {
    attention.push({ id: 'health', statusKey: 'operations.cards.health.degraded', path: '/admin' })
  }

  return { allClear: attention.length === 0, attention, healthDegradedDetails }
}
