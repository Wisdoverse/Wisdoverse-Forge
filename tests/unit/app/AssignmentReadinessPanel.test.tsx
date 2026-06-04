import { cleanup, render, screen, within } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { AssignmentReadinessPanel } from '@app/features/board/AssignmentReadinessPanel'
import type { ParticipantSummary } from '@app/shared/api/orchestration'

afterEach(cleanup)

const emptyWorkload = {
  backlog: 0,
  unassigned: 0,
  inFlight: 0,
  blocked: 0,
  review: 0,
}

describe('AssignmentReadinessPanel', () => {
  test('guides setup when no agents are connected to the work lane', () => {
    render(
      <AssignmentReadinessPanel
        participants={[]}
        workload={{ ...emptyWorkload, backlog: 2, unassigned: 2 }}
        loading={false}
        error={null}
        onRefresh={vi.fn()}
      />
    )

    const emptyState = screen.getByTestId('assignment-readiness-empty')
    expect(within(emptyState).getByText('Connect an agent before dispatch')).toBeDefined()
    expect(within(emptyState).getByText(/Agents \/ Work Lanes/)).toBeDefined()
    expect(within(emptyState).getByText(/backlog tasks cannot leave this lane/)).toBeDefined()
  })

  test('keeps connected agent chips visible when participants are available', () => {
    const participants: ParticipantSummary[] = [
      {
        id: 'participant-1',
        agentId: 'agent-1',
        name: 'Ready Agent',
        status: 'available',
        capabilities: ['codex'],
      },
    ]

    render(
      <AssignmentReadinessPanel
        participants={participants}
        workload={emptyWorkload}
        loading={false}
        error={null}
        onRefresh={vi.fn()}
      />
    )

    expect(screen.queryByTestId('assignment-readiness-empty')).toBeNull()
    expect(screen.getByText('Ready Agent')).toBeDefined()
    expect(screen.getAllByText('Available').length).toBeGreaterThan(0)
  })
})
