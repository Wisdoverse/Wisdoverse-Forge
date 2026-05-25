import { describe, expect, it } from 'vitest'
import {
  classifyPullRequest,
  renderSummary,
  summarizePullRequests,
} from '../../../scripts/lib/pr-status-summary.js'

function pr(overrides: Record<string, unknown> = {}) {
  return {
    autoMergeRequest: { enabledAt: '2026-05-25T12:00:00Z' },
    headRefName: 'codex/example',
    isDraft: false,
    mergeStateStatus: 'BLOCKED',
    number: 101,
    reviewDecision: 'REVIEW_REQUIRED',
    state: 'OPEN',
    statusCheckRollup: [
      {
        conclusion: '',
        name: 'Rust Tests',
        status: 'IN_PROGRESS',
      },
    ],
    title: 'Example PR',
    url: 'https://github.com/example/repo/pull/101',
    ...overrides,
  }
}

describe('PR status summary', () => {
  it('keeps review-required and pending-check PRs in WAIT when auto-merge is enabled', () => {
    const item = classifyPullRequest(pr())

    expect(item.status).toBe('WAIT')
    expect(item.reasons).toContain('waiting for review')
    expect(item.reasons).toContain('pending check: Rust Tests')
  })

  it('marks failed checks as ACTION', () => {
    const item = classifyPullRequest(
      pr({
        reviewDecision: 'APPROVED',
        statusCheckRollup: [
          {
            conclusion: 'FAILURE',
            name: 'Unit Tests',
            status: 'COMPLETED',
          },
        ],
      })
    )

    expect(item.status).toBe('ACTION')
    expect(item.reasons).toContain('failing check: Unit Tests')
  })

  it('marks missing auto-merge as ACTION for open PRs', () => {
    const item = classifyPullRequest(pr({ autoMergeRequest: null, statusCheckRollup: [] }))

    expect(item.status).toBe('ACTION')
    expect(item.reasons).toContain('auto-merge is not enabled')
  })

  it('marks merge conflicts as ACTION', () => {
    const item = classifyPullRequest(pr({ mergeStateStatus: 'DIRTY', reviewDecision: 'APPROVED' }))

    expect(item.status).toBe('ACTION')
    expect(item.reasons).toContain('merge conflict')
  })

  it('marks closed PRs as DONE', () => {
    const item = classifyPullRequest(pr({ state: 'MERGED' }))

    expect(item.status).toBe('DONE')
  })

  it('renders compact output without listing every waiting PR by default', () => {
    const summary = summarizePullRequests([
      pr(),
      pr({
        autoMergeRequest: null,
        number: 102,
        statusCheckRollup: [],
        title: 'Needs auto-merge',
      }),
    ])

    expect(renderSummary(summary)).toContain('ACTION 1 | WAIT 1 | DONE 0')
    expect(renderSummary(summary)).toContain('WAIT: 1 PR(s) waiting')
    expect(renderSummary(summary)).not.toContain('#101 codex/example')
  })
})
