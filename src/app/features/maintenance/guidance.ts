import type { MaintenanceFailure } from '@app/entities/maintenance'

export const maintenanceGuidance: Record<MaintenanceFailure, string> = {
  conflict:
    'This task or revision changed. Refresh its evidence before submitting again. If work is active, stop it using the task controls before recording a handoff.',
  'invalid-delivery':
    'Check the current revision, report, required text and minute limits, then submit again. A finished execution is required for run evidence; explain missing artifacts explicitly.',
  'delivery-unavailable':
    'The produced revision could not be verified. Refresh after GitHub recovers or the head change has been reviewed before recording acceptance.',
  unauthenticated:
    'Sign in again, then return to this task or retry with the same source reference.',
  forbidden:
    'Administrator access is required. Ask a Forge administrator to continue in this team space.',
  missing:
    'This task or maintenance API is unavailable. Open the task board, or ask an administrator to check the deployment.',
  destination:
    'Choose an active project and place for new tasks. Ask an administrator to check its workspace if it is still unavailable.',
  'source-closed':
    'This PR is closed or merged. Open its existing task, or choose an open PR for new work.',
  'source-base':
    'This PR targets a different branch. Choose one targeting the approved repository’s default branch, or use the ordinary task workflow.',
  'source-unavailable':
    'Forge could not read this PR. Check its number and repository access, then retry with the same number.',
  setup:
    'Open Maintenance repository in Settings and check the GitHub connection, starting version and access before retrying.',
  'invalid-request':
    'Choose a place for new tasks and check the source, title and brief before submitting again.',
  'invalid-response':
    'Forge could not verify the response. Ask an administrator to check API compatibility. Keep the same source when retrying a submission.',
  timeout:
    'This request took more than 30 seconds. Its outcome is unconfirmed. Retry a submission with the same source to recover its task, or refresh the trace.',
  cancelled:
    'The request was cancelled. Keep the same source when retrying a submission to recover its task.',
  unreachable:
    'Check your connection and try again. A submission may already have been saved; keep the same source when retrying.',
  failed:
    'Forge could not finish this request. Retry with the same source to recover a submitted task, or refresh the trace. Ask an administrator if it keeps failing.',
}
