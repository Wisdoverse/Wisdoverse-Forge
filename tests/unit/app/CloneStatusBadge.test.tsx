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
    expect(screen.getByText('Code copy waiting')).toBeInTheDocument()
    expect(screen.getByText(/Forge will start copying code soon/)).toBeInTheDocument()
    expect(screen.getByText(/status updates automatically/)).toBeInTheDocument()
    expect(screen.getByTestId('clone-status-p1')).toHaveAttribute('data-clone-status', 'queued')
  })

  it('renders the cloning status', () => {
    render(<CloneStatusBadge projectId="p1" status="cloning" variant="detail" />)
    expect(screen.getByText('Copying code…')).toBeInTheDocument()
    expect(screen.getByText(/Forge is copying code now/)).toBeInTheDocument()
    expect(screen.getByText(/You can keep working while it finishes/)).toBeInTheDocument()
  })

  it('renders the ready status without exposing source-control details', () => {
    render(
      <CloneStatusBadge
        projectId="p1"
        status="ready"
        variant="detail"
        clone={summary({ status: 'ready', resolvedBranch: 'main', headSha: 'abc1234deadbeef' })}
      />
    )
    expect(screen.getByText('Code copied')).toBeInTheDocument()
    expect(screen.getByText(/Agents can use this copied code for tasks/)).toBeInTheDocument()
    expect(screen.queryByText('main')).toBeNull()
    expect(screen.queryByText('abc1234')).toBeNull()
    expect(screen.queryByText('abc1234deadbeef')).toBeNull()
  })

  it('renders failed code access problems with beginner-safe recovery copy', () => {
    render(
      <CloneStatusBadge
        projectId="p1"
        status="failed"
        variant="detail"
        clone={summary({
          status: 'failed',
          errorClass: 'auth',
          errorMessage: 'authentication failed',
        })}
      />
    )
    expect(screen.getByText('Code copy needs help')).toBeInTheDocument()
    expect(screen.getByText(/Open Settings and Code access/)).toBeInTheDocument()
    expect(screen.getByText(/check saved access for this code project/)).toBeInTheDocument()
    expect(screen.getByText(/code website rejected Forge access/)).toBeInTheDocument()
    expect(screen.queryByText(/repository/i)).not.toBeInTheDocument()
    expect(screen.queryByText('authentication failed')).not.toBeInTheDocument()
    expect(screen.queryByText(/Code import/i)).not.toBeInTheDocument()
  })

  it('renders failed missing repositories with beginner-safe recovery copy', () => {
    render(
      <CloneStatusBadge
        projectId="p1"
        status="failed"
        variant="detail"
        clone={summary({
          status: 'failed',
          errorClass: 'not_found',
          errorMessage: 'repository not found',
        })}
      />
    )

    expect(screen.getByText(/Open Settings, then Projects/)).toBeInTheDocument()
    expect(screen.getByText(/check this project code link/)).toBeInTheDocument()
    expect(screen.getByText(/could not find this code project/)).toBeInTheDocument()
    expect(screen.queryByText('repository not found')).not.toBeInTheDocument()
    expect(screen.queryByText(/could not find this repository/i)).not.toBeInTheDocument()
  })

  it('renders network, timeout, and size failures with code-project wording', () => {
    const cases: Array<{ errorClass: CloneSummary['errorClass']; expected: RegExp }> = [
      {
        errorClass: 'network',
        expected: /Check your connection and this project code link, then choose Copy code again/i,
      },
      {
        errorClass: 'timeout',
        expected: /The code website took too long to respond/i,
      },
      {
        errorClass: 'too_large',
        expected: /This code project is too large to copy right now/i,
      },
    ]

    for (const item of cases) {
      const { unmount } = render(
        <CloneStatusBadge
          projectId="p1"
          status="failed"
          variant="detail"
          clone={summary({ status: 'failed', errorClass: item.errorClass })}
        />
      )
      expect(screen.getByText(item.expected)).toBeInTheDocument()
      expect(screen.queryByText(/repository/i)).not.toBeInTheDocument()
      unmount()
    }
  })

  it('renders failed unknown imports with a safe fallback instead of raw details', () => {
    render(
      <CloneStatusBadge
        projectId="p1"
        status="failed"
        variant="detail"
        clone={summary({ status: 'failed', errorMessage: 'unexpected git stderr' })}
      />
    )

    expect(screen.getByText(/Open Settings, then Projects/)).toBeInTheDocument()
    expect(screen.getByText(/check the code link and saved code access/)).toBeInTheDocument()
    expect(screen.queryByText('unexpected git stderr')).not.toBeInTheDocument()
  })

  it('shows the copy-again button only for the failed status', () => {
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
    expect(screen.getByRole('button', { name: /copy code again/i })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /^try again$/i })).not.toBeInTheDocument()
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

  it('disables Copy code again while the request is in flight (no double-click)', async () => {
    // A deferred promise that never resolves keeps the retry in flight, so the
    // double-click guard (`disabled={retrying}`) must hold the button disabled
    // with a `Copying code…` label until it settles.
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
    expect(retryButton).toHaveTextContent('Copying code…')
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
      expect(alert).toHaveAttribute('aria-live', 'polite')
      expect(alert).toHaveTextContent('Ask an owner or admin to let you copy code')
      expect(alert).toHaveTextContent('open Settings, then Projects')
      expect(alert).toHaveTextContent('choose Copy code again')
      expect(alert).toHaveTextContent('You do not have permission right now')
      expect(alert).not.toHaveTextContent('update project access')
      expect(alert).not.toHaveTextContent('API 403')
      expect(alert).not.toHaveTextContent('Only the owner or a manager')
    })
  })

  it('surfaces a plain retry role failure as a beginner-safe inline message', async () => {
    vi.spyOn(projectApi, 'retryClone').mockRejectedValue(new Error('owner role required'))

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
      expect(alert).toHaveTextContent('Ask an owner or admin to let you copy code')
      expect(alert).toHaveTextContent('You do not have permission right now')
      expect(alert).not.toHaveTextContent('owner role required')
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
      expect(alert).toHaveAttribute('aria-live', 'polite')
      expect(alert).toHaveTextContent('Wait a few minutes, then choose Copy code again')
      expect(alert).toHaveTextContent('for this project in the list')
      expect(alert).not.toHaveTextContent('API 500')
      expect(alert).not.toHaveTextContent('database unavailable')
    })
  })

  it('starts busy retry failures with the wait step', async () => {
    vi.spyOn(projectApi, 'retryClone').mockRejectedValue(new Error('API 429: Too many requests'))

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
      expect(alert).toHaveAttribute('aria-live', 'polite')
      expect(alert).toHaveTextContent(
        'Wait a minute, then choose Copy code again for this project in the list. Too many copy retries are happening right now.'
      )
    })
  })

  it('starts fallback retry failures with the code access check', async () => {
    vi.spyOn(projectApi, 'retryClone').mockRejectedValue(new Error('unexpected clone failure'))

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
      expect(alert).toHaveAttribute('aria-live', 'polite')
      expect(alert).toHaveTextContent('Open Settings, then Projects')
      expect(alert).toHaveTextContent('choose Copy code again for this project in the list')
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
