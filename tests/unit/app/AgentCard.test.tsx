import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { AgentCard } from '@app/features/agents/AgentCard'
import type { AgentInfo } from '@app/entities/agent'

afterEach(cleanup)

const mockAgent: AgentInfo = {
  id: 'agent-1',
  name: 'Build Runner',
  provider: 'OpenAI',
  model: 'codex',
  status: 'idle',
  tasksCompleted: 12,
  tasksInProgress: 0,
  successRate: 0.92,
  cliTool: 'codex',
  runtimeKind: 'container',
  workspaceName: 'Platform',
  projectName: 'Console',
}

describe('AgentCard', () => {
  test('explains when an idle agent can take work', () => {
    render(<AgentCard agent={mockAgent} />)

    expect(screen.getByTestId('agent-status-agent-1').textContent).toContain('Idle')
    expect(screen.getByTestId('agent-status-help-agent-1').textContent).toBe(
      'Ready for the next task'
    )
    expect(screen.getByText('Finished')).toBeDefined()
    expect(screen.getByText('Running')).toBeDefined()
    expect(screen.getByText('Success')).toBeDefined()
  })

  test('warns before assigning work to an offline agent', () => {
    render(<AgentCard agent={{ ...mockAgent, status: 'offline' }} />)

    expect(screen.getByTestId('agent-status-help-agent-1').textContent).toBe(
      'Reconnect before assigning work'
    )
  })

  test('keeps current task visible while explaining working state', () => {
    render(
      <AgentCard
        agent={{ ...mockAgent, status: 'working', tasksInProgress: 1, currentTask: 'Run tests' }}
      />
    )

    expect(screen.getByText('Run tests')).toBeDefined()
    expect(screen.getByTestId('agent-status-help-agent-1').textContent).toBe('Running a task now')
  })

  test('opens the agent detail when clicked', () => {
    const onClick = vi.fn()
    render(<AgentCard agent={mockAgent} onClick={onClick} />)

    fireEvent.click(screen.getByTestId('agent-card-agent-1'))

    expect(onClick).toHaveBeenCalledOnce()
  })
})
