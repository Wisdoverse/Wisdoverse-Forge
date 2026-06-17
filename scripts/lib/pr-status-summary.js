const ACTION_CHECK_CONCLUSIONS = new Set([
  'ACTION_REQUIRED',
  'CANCELLED',
  'ERROR',
  'FAILURE',
  'FAILED',
  'STARTUP_FAILURE',
  'TIMED_OUT',
])

const PASS_CHECK_CONCLUSIONS = new Set(['NEUTRAL', 'SKIPPED', 'SUCCESS'])
const WAIT_CHECK_STATUSES = new Set(['IN_PROGRESS', 'PENDING', 'QUEUED', 'REQUESTED', 'WAITING'])
const ACTION_MERGE_STATES = new Map([
  ['BEHIND', 'base branch changed'],
  ['DIRTY', 'merge conflict'],
])
const WAIT_STOP_LINE =
  'WAIT: stop here; use npm run pr:summary:local until cache expiry or a known remote change'
const WAIT_TOKEN_SAFE_LINE =
  'WAIT: token-safe action: do not poll in chat; use scheduled monitoring for the next check'

export function summarizePullRequests(pullRequests) {
  const items = Array.isArray(pullRequests) ? pullRequests.map(classifyPullRequest) : []
  return {
    action: items.filter((item) => item.status === 'ACTION'),
    wait: items.filter((item) => item.status === 'WAIT'),
    done: items.filter((item) => item.status === 'DONE'),
  }
}

export function classifyPullRequest(pr) {
  const failedChecks = getFailedChecks(pr.statusCheckRollup)
  const pendingChecks = getPendingChecks(pr.statusCheckRollup)
  const state = normalizeToken(pr.state)
  const reviewDecision = normalizeToken(pr.reviewDecision)
  const mergeState = normalizeToken(pr.mergeStateStatus)
  const autoMergeEnabled = Boolean(pr.autoMergeRequest)

  if (state && state !== 'OPEN') {
    return buildItem(pr, 'DONE', [`PR is ${state.toLowerCase()}`], failedChecks, pendingChecks)
  }

  const actionReasons = [
    ...missingAutoMergeReason(autoMergeEnabled, pr.isDraft),
    ...reviewActionReasons(reviewDecision),
    ...mergeActionReasons(mergeState),
    ...failedChecks.map((check) => `failing check: ${check}`),
  ]

  if (actionReasons.length > 0) {
    return buildItem(pr, 'ACTION', actionReasons, failedChecks, pendingChecks)
  }

  const waitReasons = [
    ...draftWaitReasons(pr.isDraft),
    ...reviewWaitReasons(reviewDecision),
    ...mergeWaitReasons(mergeState),
    ...pendingChecks.map((check) => `pending check: ${check}`),
  ]

  return buildItem(
    pr,
    'WAIT',
    waitReasons.length > 0 ? waitReasons : ['waiting for GitHub to merge or update status'],
    failedChecks,
    pendingChecks
  )
}

export function renderSummary(summary, options = {}) {
  const showWait = options.showWait === true
  const lines = [
    `[pr-summary] ACTION ${summary.action.length} | WAIT ${summary.wait.length} | DONE ${summary.done.length}`,
  ]

  appendActionLines(lines, summary.action)
  appendWaitLines(lines, summary.wait, showWait)
  appendDoneLines(lines, summary.done)

  return `${lines.join('\n')}\n`
}

function buildItem(pr, status, reasons, failedChecks, pendingChecks) {
  return {
    status,
    number: Number(pr.number),
    title: typeof pr.title === 'string' ? pr.title : '',
    branch: typeof pr.headRefName === 'string' ? pr.headRefName : '',
    url: typeof pr.url === 'string' ? pr.url : '',
    reasons,
    failedChecks,
    pendingChecks,
  }
}

function missingAutoMergeReason(autoMergeEnabled, isDraft) {
  if (autoMergeEnabled || isDraft) return []
  return ['auto-merge is not enabled']
}

function reviewActionReasons(reviewDecision) {
  if (reviewDecision === 'CHANGES_REQUESTED') return ['changes requested']
  return []
}

function mergeActionReasons(mergeState) {
  const reason = ACTION_MERGE_STATES.get(mergeState)
  return reason ? [reason] : []
}

function draftWaitReasons(isDraft) {
  return isDraft ? ['draft PR'] : []
}

function reviewWaitReasons(reviewDecision) {
  if (reviewDecision === 'REVIEW_REQUIRED') return ['waiting for review']
  if (reviewDecision === 'APPROVED') return ['review approved']
  return []
}

function mergeWaitReasons(mergeState) {
  if (mergeState === 'BLOCKED') return ['blocked by branch protection']
  if (mergeState === 'UNKNOWN') return ['GitHub is still calculating merge status']
  if (mergeState === 'UNSTABLE') return ['waiting for required checks']
  return []
}

function getFailedChecks(checks) {
  return normalizeChecks(checks)
    .filter((check) => ACTION_CHECK_CONCLUSIONS.has(check.outcome))
    .map((check) => check.name)
}

function getPendingChecks(checks) {
  return normalizeChecks(checks)
    .filter((check) => check.pending)
    .map((check) => check.name)
}

function normalizeChecks(checks) {
  if (!Array.isArray(checks)) return []

  return checks.map((check) => {
    const outcome = normalizeToken(check.conclusion ?? check.state)
    const status = normalizeToken(check.status ?? check.state)
    return {
      name: checkName(check),
      outcome,
      pending:
        WAIT_CHECK_STATUSES.has(status) ||
        (!PASS_CHECK_CONCLUSIONS.has(outcome) && !ACTION_CHECK_CONCLUSIONS.has(outcome)),
    }
  })
}

function checkName(check) {
  if (typeof check.name === 'string' && check.name.trim().length > 0) return check.name
  if (typeof check.context === 'string' && check.context.trim().length > 0) return check.context
  if (typeof check.workflowName === 'string' && check.workflowName.trim().length > 0) {
    return check.workflowName
  }
  return 'status check'
}

function appendActionLines(lines, items) {
  if (items.length === 0) {
    lines.push('ACTION: none')
    return
  }

  lines.push('ACTION:')
  for (const item of items) {
    lines.push(formatItemLine(item))
    lines.push(`  next: ${item.reasons.join('; ')}`)
  }
}

function appendWaitLines(lines, items, showWait) {
  if (items.length === 0) return

  if (!showWait) {
    lines.push(`WAIT: ${items.length} PR(s) waiting on review, CI, draft state, or merge queue`)
    lines.push('WAIT: use --show-wait to list them when a human needs the full queue')
    lines.push(WAIT_STOP_LINE)
    lines.push(WAIT_TOKEN_SAFE_LINE)
    return
  }

  lines.push('WAIT:')
  for (const item of items) {
    lines.push(formatItemLine(item))
    lines.push(`  reason: ${item.reasons.join('; ')}`)
  }
  lines.push(WAIT_STOP_LINE)
  lines.push(WAIT_TOKEN_SAFE_LINE)
}

function appendDoneLines(lines, items) {
  if (items.length > 0) lines.push(`DONE: ${items.length} PR(s) already closed or merged`)
}

function formatItemLine(item) {
  const branch = item.branch ? ` ${item.branch}` : ''
  const url = item.url ? ` ${item.url}` : ''
  return `- #${item.number}${branch} | ${item.title}${url}`
}

function normalizeToken(value) {
  return typeof value === 'string' ? value.trim().toUpperCase() : ''
}
