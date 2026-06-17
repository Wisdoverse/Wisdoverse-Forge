type ReviewSnapshotAction = 'load' | 'approve'

const RAW_NETWORK_ERRORS = [/^Network error$/i, /^Failed to fetch$/i]
const RAW_STATUS_ERRORS = [/^API\s+\d{3}/i, /^HTTP\s+\d{3}/i, /^Server error\s*\(\d{3}\)$/i]

const ACTION_FALLBACKS: Record<ReviewSnapshotAction, string> = {
  load: 'Refresh code fix review, then try again. Forge could not load the current GitHub review status.',
  approve:
    'Refresh code fix review, confirm build checks passed, then merge this fix again. The fix was not merged.',
}

export function reviewSnapshotErrorMessage(action: ReviewSnapshotAction, error: unknown): string {
  const detail = rawDetail(error)
  const code = statusCode(detail)
  const text = detail?.toLowerCase() ?? ''

  if (text.includes('can not approve your own pull request')) {
    return 'Ask another maintainer to review this code fix. GitHub needs someone else to review changes you opened yourself.'
  }

  if (
    code === 401 ||
    text.includes('unauthorized') ||
    text.includes('bad credentials') ||
    text.includes('sign in again')
  ) {
    return 'Sign in again, then refresh code fix review. Forge could not confirm your GitHub access.'
  }

  if (
    code === 403 ||
    text.includes('forbidden') ||
    text.includes('permission') ||
    text.includes('resource not accessible')
  ) {
    return 'Ask an owner or admin to check GitHub access for this repository, then try again.'
  }

  if (
    code === 404 ||
    text.includes('not found') ||
    text.includes('no pull request') ||
    text.includes('pull request could not be found')
  ) {
    return 'Refresh this task, then open the GitHub review again. Forge could not find the review for this task.'
  }

  if (
    text.includes('conflict') ||
    text.includes('mergeable_state') ||
    text.includes('mergeable state') ||
    text.includes('cannot be merged')
  ) {
    return 'Refresh code fix review after the branch is updated. This fix needs the latest main branch before it can merge.'
  }

  if (
    text.includes('checks') ||
    text.includes('status check') ||
    text.includes('check_suite') ||
    text.includes('required status')
  ) {
    return 'Wait for build checks to finish, then refresh code fix review before merging.'
  }

  const safeDetail = userSafeDetail(detail)
  if (safeDetail) {
    return `${ACTION_FALLBACKS[action]} ${safeDetail}`
  }

  return ACTION_FALLBACKS[action]
}

function rawDetail(error: unknown): string | null {
  if (typeof error === 'string' && error.trim()) return error.trim()
  if (error instanceof Error && error.message.trim()) return error.message.trim()
  return null
}

function statusCode(detail: string | null): number | null {
  const match = detail?.match(/\b(?:API|HTTP|Server error|GraphQL|Code:)\s*\(?(\d{3})\b/i)
  if (!match) return null
  const value = Number.parseInt(match[1] ?? '', 10)
  return Number.isFinite(value) ? value : null
}

function userSafeDetail(detail: string | null): string | null {
  if (!detail) return null
  if (RAW_NETWORK_ERRORS.some((pattern) => pattern.test(detail))) return null
  if (RAW_STATUS_ERRORS.some((pattern) => pattern.test(detail))) return null
  if (detail.length > 160) return null
  if (
    /\b(?:api|http|graphql|json|stack trace|traceback|exception|panic|stdout|stderr|database|sql|token|secret|authorization|bearer)\b/i.test(
      detail
    )
  ) {
    return null
  }
  return detail
}
