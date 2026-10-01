import { TextEncoder } from 'node:util'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MaintenanceIntake, MaintenanceTracePanel } from '@app/features/maintenance'
import { MaintenanceError } from '@app/entities/maintenance'
import { useNavigationStore } from '@app/entities/navigation'
import { useBoardStore } from '@app/entities/navigation/model/board.store'
import {
  maintenanceTrace,
  maintenanceTaskId,
  maintenanceRequestId,
  maintenanceGroupId,
} from '../../support/fixtures/maintenance'
import type { MaintenanceSubmission, MaintenanceTrace } from '@shared/types/maintenance'

const mocks = vi.hoisted(() => ({
  auth: {
    user: { id: 'admin-1', orgId: 'org-1', role: 'viewer', isAdmin: true },
    isAuthenticated: true,
    isLoading: false,
  },
  submit: vi.fn(),
  get: vi.fn(),
  navigate: vi.fn(),
}))
vi.mock('@app/shared/model/auth.context', () => ({ useAuth: () => mocks.auth }))
vi.mock('@tanstack/react-router', () => ({ useNavigate: () => mocks.navigate }))
vi.mock('@app/entities/maintenance', async (original) => ({
  ...(await original<typeof import('@app/entities/maintenance')>()),
  submitMaintenanceRequest: mocks.submit,
  getMaintenanceTrace: mocks.get,
}))
const saved = { requestId: maintenanceRequestId, taskId: maintenanceTaskId, reused: false }
beforeEach(() => {
  vi.stubGlobal('TextEncoder', TextEncoder)
  mocks.auth = {
    user: { id: 'admin-1', orgId: 'org-1', role: 'viewer', isAdmin: true },
    isAuthenticated: true,
    isLoading: false,
  }
  useNavigationStore.setState({
    selectedOrgId: 'org-1',
    selectedProjectId: 'project-1',
    agentGroups: [{ id: maintenanceGroupId, projectId: 'project-1', name: 'Maintenance' }],
  })
  useBoardStore.setState({ selectedGroupId: maintenanceGroupId })
  mocks.submit.mockResolvedValue(saved)
  mocks.get.mockResolvedValue(maintenanceTrace())
})
afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})
function fill(kind: 'request' | 'pull_request' = 'request') {
  fireEvent.click(screen.getByRole('button', { name: 'Maintenance request', exact: true }))
  if (kind === 'pull_request')
    fireEvent.change(screen.getByLabelText('Source type'), { target: { value: kind } })
  fireEvent.change(
    screen.getByLabelText(kind === 'request' ? /Stable request reference/ : /GitHub PR number/),
    { target: { value: kind === 'request' ? ' DEPENDENCY-2026-10 ' : '42' } }
  )
  fireEvent.change(screen.getByLabelText('Maintenance title'), {
    target: { value: 'Fix dependencies' },
  })
  fireEvent.change(screen.getByLabelText('Maintenance brief'), {
    target: { value: 'Run checks and report results' },
  })
}
function submit() {
  fireEvent.click(screen.getByRole('button', { name: 'Save maintenance task to wait' }))
}
function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => {
    resolve = done
  })
  return { promise, resolve }
}

describe('deliberate maintenance intake', () => {
  it('does not expose intake or read traces to a team owner without platform access', () => {
    mocks.auth.user = { ...mocks.auth.user, role: 'owner', isAdmin: false }
    render(
      <>
        <MaintenanceIntake />
        <MaintenanceTracePanel taskId={maintenanceTaskId} />
      </>
    )
    expect(screen.queryByTestId('maintenance-intake')).not.toBeInTheDocument()
    expect(mocks.submit).not.toHaveBeenCalled()
    expect(mocks.get).not.toHaveBeenCalled()
  })
  it('creates an unassigned task for a normalized stable source, then opens the returned task', async () => {
    render(<MaintenanceIntake />)
    fill()
    submit()
    expect(await screen.findByText(/Maintenance task saved to wait/)).toBeInTheDocument()
    expect(mocks.submit).toHaveBeenCalledWith(
      {
        groupId: maintenanceGroupId,
        title: 'Fix dependencies',
        brief: 'Run checks and report results',
        source: { kind: 'request', reference: 'dependency-2026-10' },
      },
      expect.any(AbortSignal)
    )
    fireEvent.click(screen.getByRole('button', { name: 'Open maintenance task' }))
    expect(mocks.navigate).toHaveBeenCalledWith({
      to: '/tasks/$taskId',
      params: { taskId: maintenanceTaskId },
    })
  })
  it('sends only a PR number and explains reused original work', async () => {
    mocks.submit.mockResolvedValueOnce({ ...saved, reused: true })
    render(<MaintenanceIntake />)
    fill('pull_request')
    submit()
    expect(await screen.findByText(/original brief and destination were kept/)).toBeInTheDocument()
    expect(mocks.submit.mock.calls[0][0].source).toEqual({ kind: 'pull_request', number: 42 })
  })
  it('prevents duplicate clicks while the submission is in flight', async () => {
    const pending = deferred<MaintenanceSubmission>()
    mocks.submit.mockReturnValueOnce(pending.promise)
    render(<MaintenanceIntake />)
    fill()
    submit()
    expect(screen.getByRole('button', { name: 'Saving maintenance task…' })).toBeDisabled()
    fireEvent.submit(
      screen.getByRole('button', { name: 'Saving maintenance task…' }).closest('form')!
    )
    expect(mocks.submit).toHaveBeenCalledTimes(1)
    await act(async () => pending.resolve(saved))
  })
  it('keeps the stable source after an uncertain timeout so deliberate retry recovers the same task', async () => {
    mocks.submit.mockRejectedValueOnce(new MaintenanceError('timeout'))
    render(<MaintenanceIntake />)
    fill()
    submit()
    expect(await screen.findByRole('alert')).toHaveTextContent('outcome is unconfirmed')
    submit()
    await screen.findByText(/Maintenance task saved to wait/)
    expect(mocks.submit.mock.calls[0][0]).toEqual(mocks.submit.mock.calls[1][0])
  })
  it('requires a place belonging to the selected project', () => {
    useNavigationStore.setState({ selectedProjectId: 'other-project' })
    render(<MaintenanceIntake />)
    fill()
    expect(screen.getByRole('button', { name: 'Save maintenance task to wait' })).toBeDisabled()
    expect(mocks.submit).not.toHaveBeenCalled()
  })
  it.each(['bad/ref', 'é'.repeat(2)])(
    'validates source %s before making a request',
    async (value) => {
      render(<MaintenanceIntake />)
      fill()
      fireEvent.change(screen.getByLabelText(/Stable request reference/), { target: { value } })
      submit()
      expect(await screen.findByRole('alert')).toBeInTheDocument()
      expect(mocks.submit).not.toHaveBeenCalled()
    }
  )
  it('counts UTF-8 bytes for the server title limit', async () => {
    render(<MaintenanceIntake />)
    fill()
    fireEvent.change(screen.getByLabelText('Maintenance title'), {
      target: { value: '界'.repeat(167) },
    })
    submit()
    expect(await screen.findByRole('alert')).toBeInTheDocument()
    expect(mocks.submit).not.toHaveBeenCalled()
  })
  it('cancels an old identity’s pending submission and never paints its late result', async () => {
    const pending = deferred<MaintenanceSubmission>()
    mocks.submit.mockReturnValueOnce(pending.promise)
    const view = render(<MaintenanceIntake />)
    fill()
    submit()
    const signal = mocks.submit.mock.calls[0][1] as AbortSignal
    mocks.auth.user = { ...mocks.auth.user, id: 'admin-2', orgId: 'org-2' }
    view.rerender(<MaintenanceIntake />)
    expect(signal.aborted).toBe(true)
    await act(async () => pending.resolve(saved))
    expect(screen.queryByRole('button', { name: 'Open maintenance task' })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Maintenance request' })).toHaveAttribute(
      'aria-expanded',
      'false'
    )
  })
})

describe('maintenance trace', () => {
  it('retains execution identifiers and timestamps independently of the task’s result state', async () => {
    const trace = maintenanceTrace()
    trace.taskState = 'failed'
    trace.attempt = 2
    trace.executions = [
      {
        runId: maintenanceRequestId,
        agentId: maintenanceGroupId,
        state: 'failed',
        startedAt: '2026-10-01T12:01:00Z',
        finishedAt: '2026-10-01T12:02:00Z',
      },
    ]
    mocks.get.mockResolvedValueOnce(trace)
    render(<MaintenanceTracePanel taskId={maintenanceTaskId} />)
    await screen.findByText(maintenanceGroupId)
    expect(screen.getByText('Execution history')).toBeInTheDocument()
    expect(screen.getAllByText('failed')).toHaveLength(2)
    expect(screen.getAllByText(maintenanceRequestId)).toHaveLength(2)
    expect(screen.queryByText(/No execution recorded/)).not.toBeInTheDocument()
    expect(screen.getByText('Finished').parentElement?.querySelector('time')).toHaveAttribute(
      'datetime',
      '2026-10-01T12:02:00Z'
    )
  })
  it('shows stored versions and the waiting task on the main readable surface', async () => {
    render(<MaintenanceTracePanel taskId={maintenanceTaskId} />)
    expect(await screen.findByText('a'.repeat(40))).toBeInTheDocument()
    expect(screen.getByText(/No execution recorded/)).toBeInTheDocument()
    expect(screen.getByText(/review those separately before using a change/)).toBeInTheDocument()
    expect(screen.getByText('release/stable')).toBeInTheDocument()
  })
  it('shows provider outage and head drift without changing stored revisions', async () => {
    const trace = maintenanceTrace()
    trace.producedPrUrl = 'https://github.com/example-org/example-repo/pull/42'
    trace.recordedPrHeadSha = 'b'.repeat(40)
    trace.producedState = {
      status: 'observed',
      checkedAt: '2026-10-01T12:05:00Z',
      headChanged: true,
      snapshot: {
        number: 42,
        url: trace.producedPrUrl,
        headSha: 'c'.repeat(40),
        state: 'open',
        baseBranch: trace.defaultBranch,
      },
    }
    trace.sourceState = {
      status: 'unavailable',
      checkedAt: '2026-10-01T12:05:00Z',
      snapshot: null,
      headChanged: false,
    }
    mocks.get.mockResolvedValueOnce(trace)
    render(<MaintenanceTracePanel taskId={maintenanceTaskId} />)
    expect(await screen.findByText(/PR version changed/)).toBeInTheDocument()
    expect(screen.getByText(/GitHub state is unavailable/)).toBeInTheDocument()
    expect(screen.getByText('b'.repeat(40))).toBeInTheDocument()
    expect(screen.getByText('c'.repeat(40))).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /merge|approve/i })).not.toBeInTheDocument()
  })
  it('removes the old trace while refreshing and on live permission refusal', async () => {
    render(<MaintenanceTracePanel taskId={maintenanceTaskId} />)
    await screen.findByText('a'.repeat(40))
    mocks.get.mockRejectedValueOnce(new MaintenanceError('forbidden'))
    fireEvent.click(screen.getByRole('button', { name: 'Refresh source and result' }))
    expect(screen.queryByText('a'.repeat(40))).not.toBeInTheDocument()
    expect(await screen.findByRole('alert')).toHaveTextContent('Administrator access is required')
  })
  it('aborts on task changes and discards the old task response', async () => {
    const previous = deferred<MaintenanceTrace | null>()
    mocks.get.mockReturnValueOnce(previous.promise)
    const view = render(<MaintenanceTracePanel taskId={maintenanceTaskId} />)
    const signal = mocks.get.mock.calls[0][1] as AbortSignal
    mocks.get.mockResolvedValueOnce(null)
    view.rerender(<MaintenanceTracePanel taskId={maintenanceRequestId} />)
    expect(signal.aborted).toBe(true)
    await act(async () => previous.resolve(maintenanceTrace()))
    await waitFor(() => expect(screen.queryByTestId('maintenance-trace')).not.toBeInTheDocument())
  })
  it('clears the displayed trace synchronously after privilege removal', async () => {
    const view = render(<MaintenanceTracePanel taskId={maintenanceTaskId} />)
    await screen.findByText('a'.repeat(40))
    mocks.auth.user = { ...mocks.auth.user, isAdmin: false }
    view.rerender(<MaintenanceTracePanel taskId={maintenanceTaskId} />)
    expect(screen.queryByTestId('maintenance-trace')).not.toBeInTheDocument()
    expect(mocks.get).toHaveBeenCalledTimes(1)
  })
})
