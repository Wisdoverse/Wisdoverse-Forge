import { MaintenanceError } from '@app/entities/maintenance'

export function deliveryGuidance(error: unknown): string {
  const reason = error instanceof MaintenanceError ? error.reason : 'failed'
  if (reason === 'forbidden')
    return 'Administrator access is required. Ask a Forge administrator to review this task.'
  if (reason === 'unauthenticated') return 'Sign in again before reviewing evidence.'
  if (reason === 'conflict')
    return 'The task or revision changed. Refresh evidence, review the new version and submit again. Stop active work using task controls before recording a handoff.'
  if (reason === 'delivery-unavailable')
    return 'The produced revision could not be verified. Refresh after GitHub recovers or the head change has been reviewed before recording acceptance.'
  if (reason === 'invalid-delivery')
    return 'Check the required fields and limits. Run evidence requires a finished execution; explain a missing change artifact explicitly.'
  if (reason === 'missing')
    return 'Open the task board to find this task in the current team space.'
  if (reason === 'invalid-response')
    return 'The response could not be verified. Ask an administrator to check API compatibility; refresh to recover saved records.'
  return 'The result is unconfirmed. Keep this submission unchanged and retry to recover its record, or refresh evidence before starting a new submission.'
}
