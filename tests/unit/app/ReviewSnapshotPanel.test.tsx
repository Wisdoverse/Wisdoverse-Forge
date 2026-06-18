import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { ReviewSnapshotPanel } from '@app/features/detail/ReviewSnapshotPanel'
import {
  orchestrationApi,
  type SelfFixReview,
  type TaskSummary,
} from '@app/shared/api/orchestration'

// The panel reads `upsertTask` off the board store; a no-op selector is enough.
vi.mock('@app/shared/model/board.store', () => ({
  useBoardStore: (selector: (s: { upsertTask: () => void }) => unknown) =>
    selector({ upsertTask: () => {} }),
}))

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

function task(overrides: Partial<TaskSummary> = {}): TaskSummary {
  return {
    id: 'task-1',
    state: 'completed',
    method: 'code_fix',
    params: { task: 'Fix the bug', message: '' },
    priority: 'normal',
    progress: 100,
    createdAt: '2026-06-16T00:00:00.000Z',
    updatedAt: '2026-06-16T00:00:00.000Z',
    selfFix: true,
    ...overrides,
  }
}

function review(overrides: Partial<SelfFixReview> = {}): SelfFixReview {
  return {
    taskId: 'task-1',
    prNumber: 42,
    prUrl: 'https://github.com/o/r/pull/42',
    diffUrl: 'https://github.com/o/r/pull/42/files',
    headSha: 'deadbeef',
    checksGreen: true,
    sensitive: false,
    reviewStatus: 'in_review',
    ...overrides,
  }
}

describe('ReviewSnapshotPanel', () => {
  it('renders the PR link and status once loaded', async () => {
    vi.spyOn(orchestrationApi, 'getSelfFixReview').mockResolvedValue(review())
    render(<ReviewSnapshotPanel task={task()} />)

    expect(await screen.findByText('Fix review')).toBeInTheDocument()
    expect(screen.queryByText(/code fix review/i)).toBeNull()
    expect(await screen.findByText('Review page #42')).toBeInTheDocument()
    expect(screen.queryByText(/GitHub review/i)).toBeNull()
    expect(screen.getByText('Waiting for review')).toBeInTheDocument()
    expect(screen.getByText('Automated checks passed')).toBeInTheDocument()
    expect(screen.queryByText(/Build checks/i)).toBeNull()
    expect(screen.getByLabelText('Refresh fix review')).toBeInTheDocument()
    expect(screen.getByText('Review the changes')).toBeInTheDocument()
    expect(screen.queryByText(/changed files/i)).toBeNull()
  })

  it('enables Approve when checks are green and the change is not sensitive', async () => {
    vi.spyOn(orchestrationApi, 'getSelfFixReview').mockResolvedValue(
      review({ checksGreen: true, sensitive: false })
    )
    const approveSpy = vi.spyOn(orchestrationApi, 'approveSelfFix').mockResolvedValue('merged')
    render(<ReviewSnapshotPanel task={task()} />)

    const button = await screen.findByTestId('review-approve')
    expect(button).not.toBeDisabled()
    expect(button).toHaveTextContent('Finish this fix')

    fireEvent.click(button)
    await waitFor(() => expect(approveSpy).toHaveBeenCalledWith('task-1'))
    // After finishing both the status row and the button can show terminal
    // wording; target the button unambiguously by its test id.
    await waitFor(() => expect(screen.getByTestId('review-approve')).toHaveTextContent('Finished'))
    expect(screen.getByTestId('review-approve')).toBeDisabled()
  })

  it('disables finishing when automated checks are not green', async () => {
    vi.spyOn(orchestrationApi, 'getSelfFixReview').mockResolvedValue(review({ checksGreen: false }))
    render(<ReviewSnapshotPanel task={task()} />)

    expect(await screen.findByTestId('review-approve')).toBeDisabled()
    expect(screen.getByText(/automated checks pass/i)).toBeInTheDocument()
    expect(screen.queryByText(/build checks/i)).toBeNull()
    expect(screen.queryByText(/merge unlocks/i)).toBeNull()
  })

  it('disables finishing until a review page exists', async () => {
    const approveSpy = vi.spyOn(orchestrationApi, 'approveSelfFix').mockResolvedValue('merged')
    vi.spyOn(orchestrationApi, 'getSelfFixReview').mockResolvedValue(
      review({ prNumber: undefined, prUrl: undefined, diffUrl: undefined, checksGreen: true })
    )
    render(<ReviewSnapshotPanel task={task()} />)

    expect(await screen.findByText(/still preparing the review page/i)).toBeInTheDocument()
    const button = screen.getByTestId('review-approve')
    expect(button).toBeDisabled()
    expect(screen.getByText(/finish after the agent opens the review page/i)).toBeInTheDocument()
    expect(screen.queryByText(/merge unlocks/i)).toBeNull()

    fireEvent.click(button)

    expect(approveSpy).not.toHaveBeenCalled()
  })

  it('disables Approve and shows a warning when the change is sensitive', async () => {
    vi.spyOn(orchestrationApi, 'getSelfFixReview').mockResolvedValue(
      review({ checksGreen: true, sensitive: true, reviewStatus: 'sensitive_blocked' })
    )
    render(<ReviewSnapshotPanel task={task()} />)

    expect(await screen.findByTestId('review-approve')).toBeDisabled()
    expect(screen.getByText('Needs owner or admin review')).toBeInTheDocument()
    expect(screen.getByText(/fix changes sensitive project areas/i)).toBeInTheDocument()
    expect(screen.queryByText(/maintainer/i)).toBeNull()
    expect(screen.queryByText(/protected files/i)).toBeNull()
  })

  it('surfaces a beginner-safe fetch error instead of raw API details', async () => {
    vi.spyOn(orchestrationApi, 'getSelfFixReview').mockRejectedValue(
      new Error('API 500: {"error":"database unavailable"}')
    )
    render(<ReviewSnapshotPanel task={task()} />)

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent(
      'Refresh fix review, then try again. Forge could not load the current review status.'
    )
    expect(alert).not.toHaveTextContent('code fix review')
    expect(alert).not.toHaveTextContent('API 500')
    expect(alert).not.toHaveTextContent('database')
  })

  it('explains when the current user cannot approve their own pull request', async () => {
    vi.spyOn(orchestrationApi, 'getSelfFixReview').mockResolvedValue(review())
    vi.spyOn(orchestrationApi, 'approveSelfFix').mockRejectedValue(
      new Error('GraphQL: Review Can not approve your own pull request (addPullRequestReview)')
    )
    render(<ReviewSnapshotPanel task={task()} />)

    fireEvent.click(await screen.findByTestId('review-approve'))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Ask another owner or admin to review this fix.')
    expect(alert).toHaveTextContent(
      'The review system needs someone else to review changes you opened yourself.'
    )
    expect(alert).not.toHaveTextContent('code host')
    expect(alert).not.toHaveTextContent('maintainer')
    expect(alert).not.toHaveTextContent('pull request')
    expect(alert).not.toHaveTextContent('GraphQL')
    expect(alert).not.toHaveTextContent('addPullRequestReview')
  })

  it('refetches the full snapshot when a pushed review-status change arrives', async () => {
    // First load = in_review; the post-merge refetch returns the fresh snapshot.
    const fetchSpy = vi
      .spyOn(orchestrationApi, 'getSelfFixReview')
      .mockResolvedValueOnce(review({ reviewStatus: 'in_review', checksGreen: true }))
      .mockResolvedValueOnce(review({ reviewStatus: 'merged', checksGreen: true }))
    const { rerender } = render(<ReviewSnapshotPanel task={task({ reviewStatus: 'in_review' })} />)

    // Initial snapshot load.
    expect(await screen.findByText('Waiting for review')).toBeInTheDocument()
    expect(fetchSpy).toHaveBeenCalledTimes(1)

    // Another operator's approve→merge arrives as an `orchestration:task_update`
    // frame → board upsert → this task prop flips to `merged`. The panel re-pulls
    // the full snapshot so every field stays consistent —
    // never stale passing-check copy next to a fresh "Merged".
    rerender(<ReviewSnapshotPanel task={task({ reviewStatus: 'merged' })} />)

    await waitFor(() => expect(screen.getByTestId('review-approve')).toHaveTextContent('Finished'))
    expect(screen.getByTestId('review-approve')).toBeDisabled()
    expect(fetchSpy).toHaveBeenCalledTimes(2)
  })
})
