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
}))
vi.mock('@app/shared/model/auth.context', () => ({ useAuth: () => mocks.auth }))
vi.mock('@app/entities/maintenance', async (original) => ({
  ...(await original<typeof import('@app/entities/maintenance')>()),
  getMaintenanceDelivery: mocks.get,
  createVerification: mocks.report,
  recordMaintenanceDecision: mocks.decision,
  recordMaintenanceHandoff: mocks.handoff,
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
