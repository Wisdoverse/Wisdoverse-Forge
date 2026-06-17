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

    expect(await screen.findByText('Pull request review')).toBeInTheDocument()
    expect(await screen.findByText('Pull request #42')).toBeInTheDocument()
    expect(screen.getByText('In review')).toBeInTheDocument()
    expect(screen.getByText('Pull request checks passing')).toBeInTheDocument()
    expect(screen.getByLabelText('Refresh pull request review')).toBeInTheDocument()
    expect(screen.getByText('Review changed files')).toBeInTheDocument()
  })

  it('enables Approve when checks are green and the change is not sensitive', async () => {
    vi.spyOn(orchestrationApi, 'getSelfFixReview').mockResolvedValue(
      review({ checksGreen: true, sensitive: false })
    )
    const approveSpy = vi.spyOn(orchestrationApi, 'approveSelfFix').mockResolvedValue('merged')
    render(<ReviewSnapshotPanel task={task()} />)

    const button = await screen.findByTestId('review-approve')
    expect(button).not.toBeDisabled()
    expect(button).toHaveTextContent('Approve and merge')

    fireEvent.click(button)
    await waitFor(() => expect(approveSpy).toHaveBeenCalledWith('task-1'))
    // After merge both the status row and the button read "Merged"; target the
    // button unambiguously by its test id.
    await waitFor(() => expect(screen.getByTestId('review-approve')).toHaveTextContent('Merged'))
    expect(screen.getByTestId('review-approve')).toBeDisabled()
  })

  it('disables Approve when pull request checks are not green', async () => {
    vi.spyOn(orchestrationApi, 'getSelfFixReview').mockResolvedValue(review({ checksGreen: false }))
    render(<ReviewSnapshotPanel task={task()} />)

    expect(await screen.findByTestId('review-approve')).toBeDisabled()
    expect(screen.getByText(/pull request checks are green/i)).toBeInTheDocument()
  })

  it('disables Approve until a pull request exists', async () => {
    const approveSpy = vi.spyOn(orchestrationApi, 'approveSelfFix').mockResolvedValue('merged')
    vi.spyOn(orchestrationApi, 'getSelfFixReview').mockResolvedValue(
      review({ prNumber: undefined, prUrl: undefined, diffUrl: undefined, checksGreen: true })
    )
    render(<ReviewSnapshotPanel task={task()} />)

    expect(await screen.findByText(/no pull request has been opened/i)).toBeInTheDocument()
    const button = screen.getByTestId('review-approve')
    expect(button).toBeDisabled()
    expect(
      screen.getByText(/approve unlocks after a pull request is available/i)
    ).toBeInTheDocument()

    fireEvent.click(button)

    expect(approveSpy).not.toHaveBeenCalled()
  })

  it('disables Approve and shows a warning when the change is sensitive', async () => {
    vi.spyOn(orchestrationApi, 'getSelfFixReview').mockResolvedValue(
      review({ checksGreen: true, sensitive: true, reviewStatus: 'sensitive_blocked' })
    )
    render(<ReviewSnapshotPanel task={task()} />)

    expect(await screen.findByTestId('review-approve')).toBeDisabled()
    expect(screen.getByText('Needs maintainer review')).toBeInTheDocument()
    expect(screen.getByText(/changes protected files/i)).toBeInTheDocument()
  })

  it('surfaces a beginner-safe fetch error instead of raw API details', async () => {
    vi.spyOn(orchestrationApi, 'getSelfFixReview').mockRejectedValue(
      new Error('API 500: {"error":"database unavailable"}')
    )
    render(<ReviewSnapshotPanel task={task()} />)

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent(
      'Refresh pull request review, then try again. Forge could not load the current pull request status.'
    )
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
    expect(alert).toHaveTextContent('Ask another maintainer to approve this pull request.')
    expect(alert).toHaveTextContent(
      'GitHub does not allow you to approve your own pull request.'
    )
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
    expect(await screen.findByText('In review')).toBeInTheDocument()
    expect(fetchSpy).toHaveBeenCalledTimes(1)

    // Another operator's approve→merge arrives as an `orchestration:task_update`
    // frame → board upsert → this task prop flips to `merged`. The panel re-pulls
    // the full snapshot so every field stays consistent —
    // never stale passing-check copy next to a fresh "Merged".
    rerender(<ReviewSnapshotPanel task={task({ reviewStatus: 'merged' })} />)

    await waitFor(() => expect(screen.getByTestId('review-approve')).toHaveTextContent('Merged'))
    expect(screen.getByTestId('review-approve')).toBeDisabled()
    expect(fetchSpy).toHaveBeenCalledTimes(2)
  })
})
