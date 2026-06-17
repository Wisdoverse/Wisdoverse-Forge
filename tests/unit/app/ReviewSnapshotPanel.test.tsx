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

    expect(await screen.findByText('Pull request #42')).toBeInTheDocument()
    expect(screen.getByText('In review')).toBeInTheDocument()
    expect(screen.getByText('CI checks passing')).toBeInTheDocument()
  })

  it('enables Approve when checks are green and the change is not sensitive', async () => {
    vi.spyOn(orchestrationApi, 'getSelfFixReview').mockResolvedValue(
      review({ checksGreen: true, sensitive: false })
    )
    const approveSpy = vi.spyOn(orchestrationApi, 'approveSelfFix').mockResolvedValue('merged')
    render(<ReviewSnapshotPanel task={task()} />)

    const button = await screen.findByTestId('review-approve')
    expect(button).not.toBeDisabled()

    fireEvent.click(button)
    await waitFor(() => expect(approveSpy).toHaveBeenCalledWith('task-1'))
    // After merge both the status row and the button read "Merged"; target the
    // button unambiguously by its test id.
    await waitFor(() => expect(screen.getByTestId('review-approve')).toHaveTextContent('Merged'))
    expect(screen.getByTestId('review-approve')).toBeDisabled()
  })

  it('disables Approve when CI is not green', async () => {
    vi.spyOn(orchestrationApi, 'getSelfFixReview').mockResolvedValue(review({ checksGreen: false }))
    render(<ReviewSnapshotPanel task={task()} />)

    expect(await screen.findByTestId('review-approve')).toBeDisabled()
  })

  it('disables Approve and shows a warning when the change is sensitive', async () => {
    vi.spyOn(orchestrationApi, 'getSelfFixReview').mockResolvedValue(
      review({ checksGreen: true, sensitive: true, reviewStatus: 'sensitive_blocked' })
    )
    render(<ReviewSnapshotPanel task={task()} />)

    expect(await screen.findByTestId('review-approve')).toBeDisabled()
    expect(screen.getByText(/sensitive path/i)).toBeInTheDocument()
  })

  it('surfaces a fetch error in a banner instead of swallowing it', async () => {
    vi.spyOn(orchestrationApi, 'getSelfFixReview').mockRejectedValue(new Error('boom'))
    render(<ReviewSnapshotPanel task={task()} />)

    expect(await screen.findByRole('alert')).toHaveTextContent('boom')
  })

  it('reflects a pushed review-status change on the task prop without refetching', async () => {
    const fetchSpy = vi
      .spyOn(orchestrationApi, 'getSelfFixReview')
      .mockResolvedValue(review({ reviewStatus: 'in_review' }))
    const { rerender } = render(<ReviewSnapshotPanel task={task({ reviewStatus: 'in_review' })} />)

    // Initial snapshot load.
    expect(await screen.findByText('In review')).toBeInTheDocument()
    expect(fetchSpy).toHaveBeenCalledTimes(1)

    // Another operator's approve→merge arrives as an `orchestration:task_update`
    // frame → board upsert → this task prop flips to `merged`.
    rerender(<ReviewSnapshotPanel task={task({ reviewStatus: 'merged' })} />)

    // The panel reflects the merge live (button reads "Merged" and is disabled)
    // and issues NO second snapshot fetch — the status syncs off the prop.
    await waitFor(() => expect(screen.getByTestId('review-approve')).toHaveTextContent('Merged'))
    expect(screen.getByTestId('review-approve')).toBeDisabled()
    expect(fetchSpy).toHaveBeenCalledTimes(1)
  })
})
