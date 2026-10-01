import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import type { SelfFixRepositorySetup } from '@shared/types/self-fix'
import { MaintenanceRepositorySection } from '@app/features/settings/MaintenanceRepositorySection'
import { RepositorySetupError } from '@app/shared/api/selfFix'

const mocks = vi.hoisted(() => ({
  auth: {
    user: { id: 'admin-1', orgId: 'org-1', role: 'viewer', isAdmin: true },
    isAuthenticated: true,
    isLoading: false,
  },
  get: vi.fn(),
}))
vi.mock('@app/shared/model/auth.context', () => ({ useAuth: () => mocks.auth }))
vi.mock('@app/shared/api/selfFix', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@app/shared/api/selfFix')>()),
  getSelfFixRepository: mocks.get,
}))

const snapshot: SelfFixRepositorySetup = {
  repository: 'example-org/example-repo',
  defaultBranch: 'release/stable',
  baseSha: 'a'.repeat(40),
  contentsWrite: true,
  pullRequestsWrite: true,
  checksRead: true,
  squashMergeAllowed: true,
}

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => {
    resolve = done
  })
  return { promise, resolve }
}

beforeEach(() => {
  mocks.auth.user = { id: 'admin-1', orgId: 'org-1', role: 'viewer', isAdmin: true }
  mocks.auth.isAuthenticated = true
  mocks.auth.isLoading = false
  mocks.get.mockResolvedValue(snapshot)
})
afterEach(cleanup)

describe('maintenance repository setup', () => {
  it('explains required access without contacting GitHub for an organization owner', () => {
    mocks.auth.user = { ...mocks.auth.user, role: 'owner', isAdmin: false }
    render(<MaintenanceRepositorySection />)
    expect(mocks.get).not.toHaveBeenCalled()
    expect(screen.getByRole('status')).toHaveTextContent('Ask a Forge administrator')
    expect(screen.queryByRole('button')).not.toBeInTheDocument()
    expect(screen.queryByTestId('maintenance-repository-snapshot')).not.toBeInTheDocument()
  })

  it('permits a platform administrator independently of their organization role', async () => {
    render(<MaintenanceRepositorySection />)
    expect(await screen.findByText(snapshot.repository)).toBeInTheDocument()
    expect(screen.getByText('release/stable')).toBeInTheDocument()
    expect(screen.getByText(snapshot.baseSha)).toBeInTheDocument()
    expect(screen.getByRole('link', { name: snapshot.repository })).toHaveAttribute(
      'href',
      `https://github.com/${snapshot.repository}`
    )
    expect(
      screen.getByText(/Each change still needs its own checks and human review/)
    ).toBeInTheDocument()
    expect(screen.queryByText(/automated checks passed/i)).not.toBeInTheDocument()
  })

  it('shows verification and merge prerequisites independently from opening a change', async () => {
    mocks.get.mockResolvedValueOnce({ ...snapshot, checksRead: false, squashMergeAllowed: false })
    render(<MaintenanceRepositorySection />)
    expect(
      await screen.findByText('Repository connected for preparing changes')
    ).toBeInTheDocument()
    expect(screen.getAllByText('Needs setup')).toHaveLength(2)
    expect(screen.getAllByText('Available')).toHaveLength(2)
    expect(screen.getByText(/Ask the repository owner to update/)).toBeInTheDocument()
  })

  it('does not declare preparation ready when a write permission is missing', async () => {
    mocks.get.mockResolvedValueOnce({ ...snapshot, contentsWrite: false })
    render(<MaintenanceRepositorySection />)
    expect(await screen.findByText('Repository access needs attention')).toBeInTheDocument()
  })

  it('shows bounded loading and prevents duplicate refresh requests', () => {
    mocks.get.mockReturnValueOnce(new Promise(() => undefined))
    render(<MaintenanceRepositorySection />)
    const button = screen.getByRole('button', { name: 'Checking connection…' })
    expect(button).toBeDisabled()
    fireEvent.click(button)
    expect(mocks.get).toHaveBeenCalledTimes(1)
    expect(screen.getByRole('status')).toHaveTextContent('up to 30 seconds')
  })

  it('clears old repository details during refresh and on a live permission refusal', async () => {
    render(<MaintenanceRepositorySection />)
    await screen.findByText(snapshot.repository)
    const pending = deferred<SelfFixRepositorySetup>()
    mocks.get.mockReturnValueOnce(pending.promise)
    fireEvent.click(screen.getByRole('button', { name: 'Check connection' }))
    expect(screen.queryByText(snapshot.repository)).not.toBeInTheDocument()
    await act(async () => pending.resolve(snapshot))
    await screen.findByText(snapshot.repository)
    mocks.get.mockRejectedValueOnce(new RepositorySetupError('forbidden'))
    fireEvent.click(screen.getByRole('button', { name: 'Check connection' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Administrator access is required')
    expect(screen.queryByTestId('maintenance-repository-snapshot')).not.toBeInTheDocument()
  })

  it('discards an old response after switching accounts', async () => {
    const previous = deferred<SelfFixRepositorySetup>()
    mocks.get.mockReturnValueOnce(previous.promise)
    const view = render(<MaintenanceRepositorySection />)
    const oldSignal = mocks.get.mock.calls[0][0] as AbortSignal
    mocks.auth.user = { ...mocks.auth.user, id: 'admin-2' }
    mocks.get.mockResolvedValueOnce({ ...snapshot, repository: 'example-org/current-repo' })
    view.rerender(<MaintenanceRepositorySection />)
    expect(oldSignal.aborted).toBe(true)
    await screen.findByText('example-org/current-repo')
    await act(async () => previous.resolve(snapshot))
    expect(screen.queryByText(snapshot.repository)).not.toBeInTheDocument()
    expect(screen.getByText('example-org/current-repo')).toBeInTheDocument()
  })

  it('clears metadata immediately when administrator access is removed', async () => {
    const view = render(<MaintenanceRepositorySection />)
    await screen.findByText(snapshot.repository)
    mocks.auth.user = { ...mocks.auth.user, isAdmin: false }
    view.rerender(<MaintenanceRepositorySection />)
    expect(screen.queryByText(snapshot.repository)).not.toBeInTheDocument()
    expect(screen.getByRole('status')).toHaveTextContent('Ask a Forge administrator')
    expect(mocks.get).toHaveBeenCalledTimes(1)
  })

  it('cancels pending work on unmount and cannot expose the late result', () => {
    mocks.get.mockReturnValueOnce(new Promise(() => undefined))
    const view = render(<MaintenanceRepositorySection />)
    const signal = mocks.get.mock.calls[0][0] as AbortSignal
    view.unmount()
    expect(signal.aborted).toBe(true)
  })

  it('shows safe error guidance and supports deliberate recovery', async () => {
    mocks.get.mockRejectedValueOnce(new Error('private.example.invalid test-installation-token'))
    render(<MaintenanceRepositorySection />)
    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('The connection check could not finish')
    expect(alert).not.toHaveTextContent('test-installation-token')
    expect(alert).not.toHaveTextContent('private.example.invalid')
    fireEvent.click(screen.getByRole('button', { name: 'Check connection' }))
    expect(await screen.findByText(snapshot.repository)).toBeInTheDocument()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
    const section = screen.getByTestId('settings-maintenance-repository')
    expect(within(section).getByRole('link', { name: 'Review agent setup' })).toHaveAttribute(
      'href',
      '/settings/runtime'
    )
  })
})
