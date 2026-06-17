import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen, waitFor, within, act } from '@testing-library/react'
import { userEvent } from '@testing-library/user-event'
import { ApprovalQueueView } from '@app/features/context/ApprovalQueueView'
import { useContextStore } from '@app/shared/model/context.store'
import type { ContextCandidateSummary } from '@shared/types/context'

const {
  listContextCandidatesMock,
  approveContextCandidateMock,
  rejectContextCandidateMock,
  subscribeMock,
  wsHandlers,
} = vi.hoisted(() => ({
  listContextCandidatesMock: vi.fn(),
  approveContextCandidateMock: vi.fn(),
  rejectContextCandidateMock: vi.fn(),
  subscribeMock: vi.fn(),
  wsHandlers: new Set<(data: unknown) => void>(),
}))

vi.mock('@app/shared/api/orchestration', () => ({
  orchestrationApi: {
    listContextCandidates: (...args: unknown[]) => listContextCandidatesMock(...args),
    approveContextCandidate: (...args: unknown[]) => approveContextCandidateMock(...args),
    rejectContextCandidate: (...args: unknown[]) => rejectContextCandidateMock(...args),
  },
}))

vi.mock('@app/shared/model/websocket.context', () => ({
  useWebSocket: () => ({
    status: 'connected',
    send: vi.fn(),
    subscribe: subscribeMock,
  }),
}))

const now = '2026-05-06T09:00:00.000Z'

function candidate(overrides: Partial<ContextCandidateSummary> = {}): ContextCandidateSummary {
  return {
    id: 'candidate-1',
    workspace_id: 'workspace-1',
    item_kind: 'memory',
    state: 'pending',
    owner_user_id: 'user-1',
    source_run_id: 'run-1',
    target_skill_id: null,
    proposed_scope_kind: 'project',
    source_available: true,
    proposed_preview: {
      title: 'Prod deploy memory',
      content_preview: 'Use make prod-ext and check service health.',
      sensitivity: 'internal',
    },
    created_at: now,
    updated_at: now,
    ...overrides,
  }
}

function mockQueue(
  queue: ContextCandidateSummary[],
  pending = queue.filter((item) => item.state === 'pending')
) {
  listContextCandidatesMock.mockImplementation((params: { limit?: number }) =>
    Promise.resolve(params.limit === 200 ? pending : queue)
  )
}

beforeEach(() => {
  wsHandlers.clear()
  subscribeMock.mockImplementation((handler: (data: unknown) => void) => {
    wsHandlers.add(handler)
    return () => wsHandlers.delete(handler)
  })
  approveContextCandidateMock.mockResolvedValue({
    candidate: { ...candidate(), state: 'approved' },
    approval: null,
    memory_item: null,
    skill: null,
  })
  rejectContextCandidateMock.mockResolvedValue({
    candidate: { ...candidate(), state: 'rejected' },
    approval: null,
    memory_item: null,
    skill: null,
  })
  mockQueue([candidate()])
  useContextStore.getState().reset()
})

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
  useContextStore.getState().reset()
})

describe('ApprovalQueueView', () => {
  test('renders candidates and disables approval when source is unavailable', async () => {
    mockQueue([
      candidate(),
      candidate({
        id: 'candidate-team-space',
        proposed_scope_kind: 'org',
        proposed_preview: {
          title: 'Team space reuse memory',
          content_preview: 'Shared setup advice for this team space.',
          sensitivity: 'internal',
        },
      }),
      candidate({
        id: 'candidate-missing',
        source_available: false,
        proposed_preview: {
          title: 'Missing source memory',
          content_preview: 'Run was not completed.',
        },
      }),
    ])

    render(<ApprovalQueueView />)

    expect(await screen.findByText('Prod deploy memory')).toBeDefined()
    expect(screen.getByText('Team space reuse memory')).toBeDefined()
    expect(screen.getAllByText('Team space').length).toBeGreaterThan(0)
    expect(screen.getByText('Who can reuse it: Team space')).toBeDefined()
    expect(screen.queryByText('Organization')).toBeNull()
    expect(screen.queryByText('Who can reuse it: Organization')).toBeNull()
    expect(screen.getByText('Missing source memory')).toBeDefined()
    expect(screen.getByTestId('context-source-unavailable-candidate-missing')).toBeDefined()
    expect(screen.getByTestId('context-approve-candidate-missing')).toBeDisabled()
    expect(useContextStore.getState().pendingCandidateCount).toBe(3)
  })

  test('passes selected filters to the list API', async () => {
    render(<ApprovalQueueView />)
    await screen.findByText('Prod deploy memory')

    await userEvent.setup().click(screen.getByRole('button', { name: 'All saved items' }))
    await userEvent.setup().selectOptions(screen.getByLabelText('Item type'), 'skill')
    await userEvent.setup().selectOptions(screen.getByLabelText('Sharing range'), 'team')

    await waitFor(() => {
      expect(listContextCandidatesMock).toHaveBeenCalledWith(
        expect.objectContaining({ state: 'all', itemKind: 'skill', scopeKind: 'team' })
      )
    })
  })

  test('approves a candidate with scope, TTL, sensitivity, and note', async () => {
    render(<ApprovalQueueView />)
    await screen.findByText('Prod deploy memory')

    await userEvent.setup().click(screen.getByTestId('context-approve-candidate-1'))
    const dialog = screen.getByRole('dialog', { name: /save prod deploy memory/i })

    expect(within(dialog).getByText(/choose who can reuse it/i)).toBeInTheDocument()
    await userEvent
      .setup()
      .selectOptions(within(dialog).getByTestId('context-approval-scope-kind'), 'team')
    expect(within(dialog).getByRole('status')).toHaveTextContent(/team reference/i)
    await userEvent.setup().type(screen.getByTestId('context-approval-scope-id'), 'team-1')
    expect(within(dialog).getByRole('status')).toHaveTextContent(
      /confirm your team can reuse this safely/i
    )
    await userEvent.setup().type(within(dialog).getByLabelText(/expiration/i), '2030-01-01T12:00')
    await userEvent
      .setup()
      .selectOptions(within(dialog).getByLabelText('Sensitivity'), 'confidential')
    await userEvent.setup().type(within(dialog).getByLabelText('Note'), 'Approved for team reuse')
    await userEvent.setup().click(
      within(dialog).getByRole('checkbox', {
        name: /i checked your team can reuse this safely/i,
      })
    )
    expect(within(dialog).getByRole('status')).toHaveTextContent(/ready to save for your team/i)
    await userEvent.setup().click(screen.getByTestId('context-approval-submit'))

    await waitFor(() => {
      expect(approveContextCandidateMock).toHaveBeenCalledWith(
        'candidate-1',
        expect.objectContaining({
          scope_kind: 'team',
          scope_id: 'team-1',
          sensitivity: 'confidential',
          reason: 'Approved for team reuse',
          confirm_expansion: true,
        })
      )
    })
    expect(approveContextCandidateMock.mock.calls[0][1].ttl_at).toContain('2030-01-01')
    await waitFor(() => expect(screen.queryByText('Prod deploy memory')).toBeNull())
  })

  test('rejects a candidate with a reason note', async () => {
    render(<ApprovalQueueView />)
    await screen.findByText('Prod deploy memory')

    await userEvent.setup().click(screen.getByTestId('context-reject-candidate-1'))
    const dialog = screen.getByRole('dialog', { name: /do not save prod deploy memory/i })
    expect(within(dialog).getByPlaceholderText(/why should this not be saved/i)).toBeInTheDocument()
    await userEvent.setup().type(within(dialog).getByTestId('context-reject-reason'), 'Too broad')
    await userEvent.setup().click(screen.getByTestId('context-reject-submit'))

    await waitFor(() => {
      expect(rejectContextCandidateMock).toHaveBeenCalledWith('candidate-1', {
        reason: 'Too broad',
      })
    })
    await waitFor(() => expect(screen.queryByText('Prod deploy memory')).toBeNull())
  })

  test('refreshes the visible queue when a websocket candidate event arrives', async () => {
    let queue = [candidate()]
    listContextCandidatesMock.mockImplementation((params: { limit?: number }) =>
      Promise.resolve(params.limit === 200 ? queue : queue)
    )

    render(<ApprovalQueueView />)
    await screen.findByText('Prod deploy memory')

    queue = [
      candidate(),
      candidate({
        id: 'candidate-2',
        proposed_preview: {
          title: 'Realtime skill candidate',
          content_preview: 'A candidate arrived over WebSocket.',
        },
      }),
    ]

    await act(async () => {
      for (const handler of wsHandlers) {
        handler({ type: 'context_candidate.created', candidateId: 'candidate-2' })
      }
    })

    expect(await screen.findByText('Realtime skill candidate')).toBeDefined()
  })
})
