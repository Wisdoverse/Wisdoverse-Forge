import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { CloneStatusBadge } from '@app/features/manage-project'
import { projectApi, type CloneSummary } from '@app/entities/project'

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

function summary(overrides: Partial<CloneSummary> = {}): CloneSummary {
  return {
    status: overrides.status ?? 'ready',
    attempt: overrides.attempt ?? 1,
    updatedAt: overrides.updatedAt ?? '2026-06-15T00:00:00.000Z',
    resolvedBranch: overrides.resolvedBranch,
    headSha: overrides.headSha,
    errorClass: overrides.errorClass,
    errorMessage: overrides.errorMessage,
  }
}

describe('CloneStatusBadge', () => {
  it('renders nothing for the none status (no repository)', () => {
    const { container } = render(<CloneStatusBadge projectId="p1" status="none" variant="detail" />)
    expect(container).toBeEmptyDOMElement()
  })

  it('renders the queued status', () => {
    render(<CloneStatusBadge projectId="p1" status="queued" variant="detail" />)
    expect(screen.getByText('Code import queued')).toBeInTheDocument()
    expect(screen.getByTestId('clone-status-p1')).toHaveAttribute('data-clone-status', 'queued')
  })

  it('renders the cloning status', () => {
    render(<CloneStatusBadge projectId="p1" status="cloning" variant="detail" />)
    expect(screen.getByText('Copying code…')).toBeInTheDocument()
  })

  it('renders the ready status with branch and short head sha', () => {
    render(
      <CloneStatusBadge
        projectId="p1"
        status="ready"
        variant="detail"
        clone={summary({ status: 'ready', resolvedBranch: 'main', headSha: 'abc1234deadbeef' })}
      />
    )
    expect(screen.getByText('Code ready')).toBeInTheDocument()
    expect(screen.getByText('main')).toBeInTheDocument()
    expect(screen.getByText('abc1234')).toBeInTheDocument()
  })

  it('renders the failed status with the redacted error message', () => {
    render(
      <CloneStatusBadge
        projectId="p1"
        status="failed"
        variant="detail"
        clone={summary({ status: 'failed', errorMessage: 'authentication failed' })}
      />
    )
    expect(screen.getByText('Code import failed')).toBeInTheDocument()
    expect(screen.getByText('authentication failed')).toBeInTheDocument()
  })

  it('shows the Retry button only for the failed status', () => {
    const { rerender } = render(
      <CloneStatusBadge projectId="p1" status="ready" variant="detail" clone={summary()} />
    )
    expect(screen.queryByTestId('clone-retry-p1')).not.toBeInTheDocument()

    rerender(
      <CloneStatusBadge
        projectId="p1"
        status="failed"
        variant="detail"
        clone={summary({ status: 'failed', errorMessage: 'boom' })}
      />
    )
    expect(screen.getByTestId('clone-retry-p1')).toBeInTheDocument()
  })

  it('calls retryClone and reports the new attempt via onRetried', async () => {
    const next = summary({ status: 'queued', attempt: 2 })
    const retrySpy = vi.spyOn(projectApi, 'retryClone').mockResolvedValue(next)
    const onRetried = vi.fn()

    render(
      <CloneStatusBadge
        projectId="p1"
        status="failed"
        variant="detail"
        clone={summary({ status: 'failed', errorMessage: 'boom' })}
        onRetried={onRetried}
      />
    )

    fireEvent.click(screen.getByTestId('clone-retry-p1'))

    await waitFor(() => expect(retrySpy).toHaveBeenCalledWith('p1'))
    await waitFor(() => expect(onRetried).toHaveBeenCalledWith(next))
  })

  it('disables Try again while the request is in flight (no double-click)', async () => {
    // A deferred promise that never resolves keeps the retry in flight, so the
    // double-click guard (`disabled={retrying}`) must hold the button disabled
    // with a `Trying…` label until it settles.
    vi.spyOn(projectApi, 'retryClone').mockReturnValue(new Promise<CloneSummary>(() => {}))

    render(
      <CloneStatusBadge
        projectId="p1"
        status="failed"
        variant="detail"
        clone={summary({ status: 'failed', errorMessage: 'boom' })}
      />
    )

    const retryButton = screen.getByTestId('clone-retry-p1')
    fireEvent.click(retryButton)

    await waitFor(() => expect(retryButton).toBeDisabled())
    expect(retryButton).toHaveTextContent('Trying…')
  })

  it('surfaces a retry permission failure as a beginner-safe inline message', async () => {
    vi.spyOn(projectApi, 'retryClone').mockRejectedValue(
      new Error('API 403: Only the owner or a manager can retry this clone')
    )

    render(
      <CloneStatusBadge
        projectId="p1"
        status="failed"
        variant="detail"
        clone={summary({ status: 'failed', errorMessage: 'boom' })}
      />
    )

    fireEvent.click(screen.getByTestId('clone-retry-p1'))

    await waitFor(() => {
      const alert = screen.getByRole('alert')
      expect(alert).toHaveTextContent('Ask an owner or admin to let you try again')
      expect(alert).not.toHaveTextContent('update project access')
      expect(alert).not.toHaveTextContent('API 403')
      expect(alert).not.toHaveTextContent('Only the owner or a manager')
    })
  })

  it('does not show raw server details when retrying clone fails', async () => {
    vi.spyOn(projectApi, 'retryClone').mockRejectedValue(
      new Error('API 500: database unavailable while updating clone attempt')
    )

    render(
      <CloneStatusBadge
        projectId="p1"
        status="failed"
        variant="detail"
        clone={summary({ status: 'failed', errorMessage: 'boom' })}
      />
    )

    fireEvent.click(screen.getByTestId('clone-retry-p1'))

    await waitFor(() => {
      const alert = screen.getByRole('alert')
      expect(alert).toHaveTextContent('Forge could not copy code right now')
      expect(alert).not.toHaveTextContent('API 500')
      expect(alert).not.toHaveTextContent('database unavailable')
    })
  })

  it('renders an icon-only indicator in the compact variant', () => {
    render(<CloneStatusBadge projectId="p1" status="cloning" variant="compact" />)
    const badge = screen.getByTestId('clone-status-p1')
    expect(badge).toHaveAttribute('title', 'Copying code…')
    // Compact mode shows no label text and no retry affordance.
    expect(screen.queryByText('Copying code…')).not.toBeInTheDocument()
    expect(screen.queryByTestId('clone-retry-p1')).not.toBeInTheDocument()
  })
})
