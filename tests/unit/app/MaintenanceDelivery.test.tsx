import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MaintenanceDeliveryPanel } from '@app/features/maintenance-delivery'
import { MaintenanceError } from '@app/entities/maintenance'
import { useNavigationStore } from '@app/entities/navigation'
import { maintenanceTaskId } from '../../support/fixtures/maintenance'
import {
  maintenanceDelivery,
  maintenanceDeliveryDecision,
  maintenanceDeliveryReport,
} from '../../support/fixtures/maintenance-delivery'
import type { MaintenanceDelivery } from '@shared/types/maintenance-delivery'

const mocks = vi.hoisted(() => ({
  auth: {
    user: { id: 'admin-1', orgId: 'org-1', isAdmin: true },
    isAuthenticated: true,
    isLoading: false,
  },
  get: vi.fn(),
  report: vi.fn(),
  decision: vi.fn(),
  handoff: vi.fn(),
  reconcile: vi.fn(),
}))
vi.mock('@app/shared/model/auth.context', () => ({ useAuth: () => mocks.auth }))
vi.mock('@app/entities/maintenance', async (original) => ({
  ...(await original<typeof import('@app/entities/maintenance')>()),
  getMaintenanceDelivery: mocks.get,
  createVerification: mocks.report,
  recordMaintenanceDecision: mocks.decision,
  recordMaintenanceHandoff: mocks.handoff,
  reconcileMaintenanceHead: mocks.reconcile,
}))
beforeEach(() => {
  mocks.auth = {
    user: { id: 'admin-1', orgId: 'org-1', isAdmin: true },
    isAuthenticated: true,
    isLoading: false,
  }
  useNavigationStore.setState({ selectedOrgId: 'org-1' })
  mocks.get.mockResolvedValue(maintenanceDelivery())
  mocks.report.mockResolvedValue(maintenanceDeliveryReport())
  mocks.decision.mockResolvedValue(maintenanceDeliveryDecision())
})
afterEach(cleanup)
function fillReport() {
  fireEvent.change(screen.getByLabelText('Allowed scope'), { target: { value: 'one module' } })
  fireEvent.change(screen.getByLabelText('Acceptance criteria'), {
    target: { value: 'tests pass' },
  })
  fireEvent.change(screen.getByLabelText('Environment and constraints'), {
    target: { value: 'same fixture' },
  })
  fireEvent.change(screen.getByLabelText('Changes and known failures'), {
    target: { value: 'updated one module' },
  })
}
describe('maintenance evidence operator workflow', () => {
  it('requires live platform-admin state before loading records', () => {
    mocks.auth.user.isAdmin = false
    render(<MaintenanceDeliveryPanel taskId={maintenanceTaskId} />)
    expect(mocks.get).not.toHaveBeenCalled()
    expect(screen.queryByTestId('maintenance-delivery')).not.toBeInTheDocument()
  })
  it('keeps reported checks, timestamped observations and human acceptance visibly separate', async () => {
    render(<MaintenanceDeliveryPanel taskId={maintenanceTaskId} />)
    await screen.findByTestId('verification-report')
    expect(screen.getByText('Current GitHub observation')).toBeInTheDocument()
    expect(
      screen.getByText('GitHub checks observed when this report was saved')
    ).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Human verdict' })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Review the exact change' })).toHaveAttribute(
      'href',
      `https://github.com/${maintenanceDeliveryReport().snapshot.repository}/compare/${maintenanceDeliveryReport().startingRevision}...${maintenanceDeliveryReport().revision}`
    )
    expect(mocks.decision).not.toHaveBeenCalled()
  })
  it('reuses an unchanged submission reference after an unconfirmed response', async () => {
    mocks.report.mockRejectedValueOnce(new MaintenanceError('unreachable'))
    render(<MaintenanceDeliveryPanel taskId={maintenanceTaskId} />)
    await screen.findByTestId('verification-report')
    fillReport()
    const form = screen.getByLabelText('Allowed scope').closest('form')!
    fireEvent.submit(form)
    await screen.findByText(/The result is unconfirmed/)
    fireEvent.submit(form)
    await waitFor(() => expect(mocks.report).toHaveBeenCalledTimes(2))
    expect(mocks.report.mock.calls[0][1].requestKey).toEqual(
      mocks.report.mock.calls[1][1].requestKey
    )
    expect(mocks.report.mock.calls[1][1]).toMatchObject({
      expectedVersion: 3,
      expectedRevision: maintenanceDelivery().currentRevision,
      runId: maintenanceDelivery().latestRunId,
    })
  })
  it('explains that a verdict waits while a Forge merge is unconfirmed', async () => {
    mocks.decision.mockRejectedValueOnce(new MaintenanceError('merge-in-progress'))
    render(<MaintenanceDeliveryPanel taskId={maintenanceTaskId} />)
    await screen.findByTestId('verification-report')
    fireEvent.change(screen.getByLabelText('Verdict reason and quality findings'), {
      target: { value: 'needs more review' },
    })
    fireEvent.submit(screen.getByLabelText('Verdict reason and quality findings').closest('form')!)
    expect(
      await screen.findByText(/A merge of this change is in progress or unconfirmed/)
    ).toHaveTextContent('Choose Finish this fix')
  })
  it('keeps omitted human time unknown and preserves explicit zero in a verdict', async () => {
    render(<MaintenanceDeliveryPanel taskId={maintenanceTaskId} />)
    await screen.findByTestId('verification-report')
    fireEvent.change(screen.getByLabelText('Verdict reason and quality findings'), {
      target: { value: 'needs more review' },
    })
    fireEvent.change(screen.getByLabelText('review minutes'), { target: { value: '0' } })
    for (const category of ['setup', 'handling', 'recovery', 'rework', 'operation'])
      fireEvent.change(screen.getByLabelText(`${category} minutes`), { target: { value: '' } })
    fireEvent.submit(screen.getByLabelText('Verdict reason and quality findings').closest('form')!)
    await waitFor(() => expect(mocks.decision).toHaveBeenCalledTimes(1))
    expect(mocks.decision.mock.calls[0][1].humanMinutes).toEqual({
      setup: null,
      handling: null,
      review: 0,
      recovery: null,
      rework: null,
      operation: null,
    })
  })
  it('offers the newer pull-request revision with both revisions and a confirmation step', async () => {
    const data = maintenanceDelivery()
    const stored = data.trace.recordedPrHeadSha!
    const next = 'd'.repeat(40)
    data.trace.producedState.headChanged = true
    data.trace.producedState.snapshot!.headSha = next
    mocks.get.mockResolvedValue(data)
    mocks.reconcile.mockResolvedValue({
      taskId: maintenanceTaskId,
      previousHead: stored,
      currentHead: next,
      changed: true,
      sensitive: false,
      reviewStatus: 'in_review',
    })
    render(<MaintenanceDeliveryPanel taskId={maintenanceTaskId} />)
    const action = await screen.findByRole('button', { name: 'Use the new revision' })
    const group = screen.getByTestId('maintenance-reconcile-head')
    expect(group).toHaveTextContent(`Recorded revision ${stored.slice(0, 7)}`)
    expect(group).toHaveTextContent(`newer revision on GitHub ${next.slice(0, 7)}`)
    expect(action).toHaveAccessibleDescription(
      'Earlier reports and verdicts no longer apply; a new report is needed before acceptance.'
    )
    fireEvent.click(action)
    const confirm = screen.getByRole('button', {
      name: `Confirm: use revision ${next.slice(0, 7)}`,
    })
    await waitFor(() => expect(confirm).toHaveFocus())
    expect(mocks.reconcile).not.toHaveBeenCalled()
    fireEvent.click(confirm)
    await waitFor(() => expect(mocks.reconcile).toHaveBeenCalledTimes(1))
    expect(mocks.reconcile.mock.calls[0][0]).toBe(maintenanceTaskId)
    expect(mocks.reconcile.mock.calls[0][1]).toEqual({
      expectedStoredHead: stored,
      expectedNewHead: next,
    })
    const notice = await screen.findByText(/The newer revision is now recorded/)
    expect(notice).toHaveAttribute('role', 'status')
    await waitFor(() => expect(notice).toHaveFocus())
    expect(mocks.get).toHaveBeenCalledTimes(2)
  })
  it('cancels the confirmation without writing', async () => {
    const data = maintenanceDelivery()
    data.trace.producedState.headChanged = true
    data.trace.producedState.snapshot!.headSha = 'd'.repeat(40)
    mocks.get.mockResolvedValue(data)
    render(<MaintenanceDeliveryPanel taskId={maintenanceTaskId} />)
    fireEvent.click(await screen.findByRole('button', { name: 'Use the new revision' }))
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(screen.getByRole('button', { name: 'Use the new revision' })).toBeInTheDocument()
    expect(mocks.reconcile).not.toHaveBeenCalled()
  })
  it.each([
    ['the head did not change', (d: MaintenanceDelivery) => d],
    [
      'a merge is in progress or unconfirmed',
      (d: MaintenanceDelivery) => ({ ...d, recovery: { ...d.recovery, reviewStatus: 'approved' } }),
    ],
    [
      'the change is merged',
      (d: MaintenanceDelivery) => ({ ...d, recovery: { ...d.recovery, reviewStatus: 'merged' } }),
    ],
    [
      'the pull request is closed',
      (d: MaintenanceDelivery) => {
        d.trace.producedState.snapshot!.state = 'closed'
        return d
      },
    ],
  ])('does not offer the newer revision when %s', async (name, shape) => {
    const data = maintenanceDelivery()
    if (name !== 'the head did not change') {
      data.trace.producedState.headChanged = true
      data.trace.producedState.snapshot!.headSha = 'd'.repeat(40)
    }
    mocks.get.mockResolvedValue(shape(data))
    render(<MaintenanceDeliveryPanel taskId={maintenanceTaskId} />)
    await screen.findByTestId('verification-report')
    expect(screen.queryByTestId('maintenance-reconcile-head')).toBeNull()
  })
  it('explains a refused newer revision in plain language', async () => {
    const data = maintenanceDelivery()
    data.trace.producedState.headChanged = true
    data.trace.producedState.snapshot!.headSha = 'd'.repeat(40)
    mocks.get.mockResolvedValue(data)
    mocks.reconcile.mockRejectedValueOnce(new MaintenanceError('reconcile-head-moved'))
    render(<MaintenanceDeliveryPanel taskId={maintenanceTaskId} />)
    fireEvent.click(await screen.findByRole('button', { name: 'Use the new revision' }))
    fireEvent.click(screen.getByRole('button', { name: /Confirm: use revision/ }))
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'The pull request moved to another revision. Refresh evidence to see the newest revision, then try again.'
    )
  })
  it('disables acceptance on a changed head and handoff while work is active', async () => {
    const data = maintenanceDelivery()
    data.currentChecks.status = 'head_changed'
    data.recovery.taskState = 'working'
    mocks.get.mockResolvedValue(data)
    render(<MaintenanceDeliveryPanel taskId={maintenanceTaskId} />)
    await screen.findByTestId('verification-report')
    expect(screen.getByRole('option', { name: 'Accepted', hidden: true })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Save handoff', hidden: true })).toBeDisabled()
  })
  it('requires a new report when the latest report is stale even if older evidence matches', async () => {
    const data = maintenanceDelivery()
    const latest = maintenanceDeliveryReport()
    latest.id = '99999999-9999-4999-8999-999999999999'
    latest.revision = 'c'.repeat(40)
    data.reports.unshift(latest)
    mocks.get.mockResolvedValue(data)
    render(<MaintenanceDeliveryPanel taskId={maintenanceTaskId} />)
    await screen.findByText(/Create a report for the current recorded run and revision/)
    expect(screen.queryByLabelText('Verdict reason and quality findings')).not.toBeInTheDocument()
    expect(screen.getAllByTestId('verification-report')).toHaveLength(2)
  })
  it('aborts and discards prior team-space reads before a new space renders', async () => {
    let resolve!: (data: MaintenanceDelivery) => void
    const pending = new Promise<MaintenanceDelivery>((done) => {
      resolve = done
    })
    mocks.get.mockReturnValueOnce(pending)
    render(<MaintenanceDeliveryPanel taskId={maintenanceTaskId} />)
    await waitFor(() => expect(mocks.get).toHaveBeenCalledTimes(1))
    const signal = mocks.get.mock.calls[0][1] as AbortSignal
    act(() => useNavigationStore.setState({ selectedOrgId: 'org-2' }))
    await screen.findByTestId('verification-report')
    expect(signal.aborted).toBe(true)
    const old = maintenanceDelivery()
    old.reports[0].snapshot.changeSummary = 'Prior team private draft'
    await act(async () => resolve(old))
    expect(screen.queryByText('Prior team private draft')).not.toBeInTheDocument()
  })
})
