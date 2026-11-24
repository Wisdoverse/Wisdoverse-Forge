import { MaintenanceError } from '@app/entities/maintenance'

export function deliveryGuidance(error: unknown): string {
  const reason = error instanceof MaintenanceError ? error.reason : 'failed'
  if (reason === 'forbidden')
    return 'Administrator access is required. Ask a Forge administrator to review this task.'
  if (reason === 'unauthenticated') return 'Sign in again before reviewing evidence.'
  if (reason === 'conflict')
    return 'The task or revision changed. Refresh evidence, review the new version and submit again. Stop active work using task controls before recording a handoff.'
  if (reason === 'busy')
    return 'Another operation on this change is in progress. Wait a moment, refresh evidence, then try again.'
  if (reason === 'reconcile-stale')
    return 'The recorded revision changed since this page loaded. Refresh evidence, check both revisions, then try again.'
  if (reason === 'reconcile-head-moved')
    return 'The pull request moved to another revision. Refresh evidence to see the newest revision, then try again.'
  if (reason === 'reconcile-not-open')
    return 'The pull request is not open on GitHub. Reopen it there, or finish the change on GitHub.'
  if (reason === 'reconcile-base-changed')
    return 'The pull request no longer targets the branch this task was opened for. Finish the change on GitHub.'
  if (reason === 'reconcile-merged')
    return 'This change is already merged. There is no newer revision to use.'
  if (reason === 'reconcile-merge-unconfirmed')
    return 'A finish attempt for this change is in progress or not confirmed. Choose Finish this fix to complete it, then use the new revision.'
  if (reason === 'provider-unavailable')
    return 'GitHub could not be read, so nothing was changed. Try again in a moment.'
  if (reason === 'merge-in-progress')
    return "A merge of this change is in progress or unconfirmed. Choose Finish this fix in the task's fix check status to complete it, then record the verdict."
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
