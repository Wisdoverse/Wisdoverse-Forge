import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import { userEvent } from '@testing-library/user-event'
import { ApprovalQueueView } from '@app/features/context/ApprovalQueueView'
import { orchestrationApi } from '@app/shared/api/orchestration'
import { useContextStore } from '@app/shared/model/context.store'
import type { ContextCandidateSummary } from '@shared/types/context'

const subscribeMock = vi.hoisted(() => vi.fn(() => vi.fn()))

vi.mock('@app/shared/model/websocket.context', () => ({
  useWebSocket: () => ({
    status: 'connected',
    send: vi.fn(),
    subscribe: subscribeMock,
  }),
}))

vi.mock('@app/shared/api/orchestration', async () => {
  const actual = await vi.importActual<typeof import('@app/shared/api/orchestration')>(
    '@app/shared/api/orchestration'
  )
  return {
    ...actual,
    orchestrationApi: {
      ...actual.orchestrationApi,
      listContextCandidates: vi.fn(),
      approveContextCandidate: vi.fn(),
      rejectContextCandidate: vi.fn(),
    },
  }
})

const listContextCandidates = vi.mocked(orchestrationApi.listContextCandidates)
const approveContextCandidate = vi.mocked(orchestrationApi.approveContextCandidate)
const rejectContextCandidate = vi.mocked(orchestrationApi.rejectContextCandidate)

const candidate: ContextCandidateSummary = {
  id: 'candidate-1',
  workspace_id: 'workspace-1',
  item_kind: 'memory',
  state: 'pending',
  owner_user_id: 'user-1',
  source_run_id: 'run-1',
  target_skill_id: null,
  proposed_scope_kind: 'user',
  source_available: true,
  proposed_preview: {
    title: 'Use stable credentials',
    content_preview: 'Remember to rotate repository tokens safely.',
    sensitivity: 'internal',
  },
  created_at: '2026-05-20T12:00:00.000Z',
  updated_at: '2026-05-20T12:00:00.000Z',
}

beforeEach(() => {
  subscribeMock.mockClear()
  useContextStore.getState().reset()
  listContextCandidates.mockResolvedValue([candidate])
  approveContextCandidate.mockResolvedValue({
    candidate: { ...candidate, state: 'approved' },
    item: null,
  })
  rejectContextCandidate.mockResolvedValue({
    candidate: { ...candidate, state: 'rejected' },
  })
})

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
  useContextStore.getState().reset()
})

describe('ApprovalQueueView', () => {
  test('guides operators through approval decisions and the approve panel', async () => {
    render(<ApprovalQueueView />)

    expect(await screen.findByTestId('context-approval-path')).toBeDefined()
    expect(screen.getByText('Approval path')).toBeDefined()
    expect(screen.getByText(/choose the smallest safe sharing range/i)).toBeDefined()
    expect(await screen.findByText('Use stable credentials')).toBeDefined()
    expect(screen.getByText('Saved memory')).toBeDefined()
    expect(screen.getAllByText('User only').length).toBeGreaterThan(0)
    expect(screen.getByText('Suggested for your own account')).toBeDefined()
    expect(screen.getByText('Original task available')).toBeDefined()
    expect(screen.queryByText(/^Workspace /)).toBeNull()
    expect(screen.queryByText(/^Owner /)).toBeNull()
    expect(screen.queryByText(/^Run /)).toBeNull()

    await userEvent.setup().click(screen.getByTestId('context-approve-candidate-1'))

    expect(screen.getByTestId('context-decision-checklist')).toBeDefined()
    expect(screen.getByText('Approve only when')).toBeDefined()
    expect(screen.getByText(/sharing range is no wider than the people who need it/i)).toBeDefined()
    expect(screen.getByText(/sensitivity and redaction match the content/i)).toBeDefined()
    expect(screen.getByText('Who can reuse it')).toBeDefined()
    expect(screen.getByText(/exact ID from settings/i)).toBeDefined()
  })

  test('explains how to recover from empty approval filters', async () => {
    listContextCandidates.mockResolvedValue([])

    render(<ApprovalQueueView />)

    await waitFor(() => expect(listContextCandidates).toHaveBeenCalled())
    expect(await screen.findByText('No items match these filters')).toBeDefined()
    expect(screen.getByText(/switch status to all or clear item and reuse filters/i)).toBeDefined()
  })

  test('shows beginner network guidance when the approval queue cannot load', async () => {
    listContextCandidates.mockRejectedValueOnce(new TypeError('Failed to fetch'))

    render(<ApprovalQueueView />)

    const error = await screen.findByTestId('context-approval-error')
    expect(error.textContent).toContain('approval queue could not load')
    expect(error.textContent).toContain('app could not reach the service')
    expect(error.textContent).not.toMatch(/failed to fetch/i)
  })

  test('shows beginner conflict guidance when approving fails', async () => {
    approveContextCandidate.mockRejectedValueOnce(new Error('409 conflict'))

    render(<ApprovalQueueView />)

    await screen.findByText('Use stable credentials')
    await userEvent.setup().click(screen.getByTestId('context-approve-candidate-1'))
    await userEvent.setup().click(screen.getByTestId('context-approval-submit'))

    const error = await screen.findByTestId('context-approval-error')
    expect(error.textContent).toContain('item changed while you were reviewing it')
    expect(error.textContent).not.toContain('Code:')
    expect(error.textContent).not.toContain('409 conflict')
  })

  test('shows beginner permission guidance when rejecting fails', async () => {
    rejectContextCandidate.mockRejectedValueOnce(new Error('403 Forbidden'))

    render(<ApprovalQueueView />)

    await screen.findByText('Use stable credentials')
    await userEvent.setup().click(screen.getByTestId('context-reject-candidate-1'))
    await userEvent.setup().click(screen.getByTestId('context-reject-submit'))

    const error = await screen.findByTestId('context-approval-error')
    expect(error.textContent).toContain('do not have permission')
    expect(error.textContent).toContain('owner or admin')
    expect(error.textContent).not.toContain('403 Forbidden')
  })
})
