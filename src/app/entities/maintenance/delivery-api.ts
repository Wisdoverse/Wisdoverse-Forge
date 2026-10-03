import type {
  DecisionInput,
  HandoffInput,
  OutcomeQuery,
  VerificationInput,
} from '@shared/types/maintenance-delivery'
import { exchange, MaintenanceError } from './api'
import {
  comparison,
  decision,
  delivery,
  handoff,
  outcomes,
  report,
  taskIdentity,
} from './delivery-decoder'

function taskPath(id: string, resource: string) {
  taskIdentity(id)
  return `/api/v1/self-fix/tasks/${id}/${resource}`
}
export function getMaintenanceDelivery(id: string, signal?: AbortSignal) {
  return exchange(taskPath(id, 'delivery'), { method: 'GET' }, (v) => delivery(v, id), signal)
}
export function createVerification(id: string, input: VerificationInput, signal?: AbortSignal) {
  return exchange(
    taskPath(id, 'reports'),
    { method: 'POST', body: JSON.stringify(input) },
    (v) => {
      const saved = report(v)
      if (
        saved.taskId !== id ||
        saved.revision !== input.expectedRevision ||
        saved.runId !== (input.runId ?? null)
      )
        throw new MaintenanceError('invalid-response')
      return saved
    },
    signal
  )
}
export function recordMaintenanceDecision(id: string, input: DecisionInput, signal?: AbortSignal) {
  return exchange(
    taskPath(id, 'decisions'),
    { method: 'POST', body: JSON.stringify(input) },
    (v) => {
      const saved = decision(v)
      if (
        saved.reportId !== input.reportId ||
        saved.verdict !== input.verdict ||
        (saved.github && saved.github.revision !== input.expectedRevision)
      )
        throw new MaintenanceError('invalid-response')
      return saved
    },
    signal
  )
}
export function recordMaintenanceHandoff(id: string, input: HandoffInput, signal?: AbortSignal) {
  return exchange(
    taskPath(id, 'handoffs'),
    { method: 'POST', body: JSON.stringify(input) },
    (v) => {
      const saved = handoff(v)
      if (saved.taskId !== id || saved.snapshot.revision !== input.expectedRevision)
        throw new MaintenanceError('invalid-response')
      return saved
    },
    signal
  )
}
export function getMaintenanceOutcomes(query: OutcomeQuery = {}, signal?: AbortSignal) {
  const params = new URLSearchParams()
  for (const [key, value] of Object.entries(query))
    if (value !== null && value !== undefined) params.set(key, value)
  return exchange(`/api/v1/self-fix/outcomes?${params}`, { method: 'GET' }, outcomes, signal)
}
export function compareMaintenanceReports(ids: string[], signal?: AbortSignal) {
  ids.forEach(taskIdentity)
  return exchange(
    `/api/v1/self-fix/comparison?${new URLSearchParams({ reportIds: ids.join(',') })}`,
    { method: 'GET' },
    (v) => comparison(v, ids),
    signal
  )
}
