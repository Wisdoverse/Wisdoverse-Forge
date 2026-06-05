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

  test('summarizes managed workspace agents without raw provider/model pairs', () => {
    render(<AgentCard agent={mockAgent} />)

    expect(screen.getByText('Codex')).toBeDefined()
    expect(screen.getByText('Managed workspace')).toBeDefined()
    expect(screen.getByText('Console')).toBeDefined()
    expect(screen.queryByText('OpenAI · codex')).toBeNull()
  })

  test('labels agents joined from this computer in beginner language', () => {
    render(
      <AgentCard
        agent={{
          ...mockAgent,
          id: 'local-agent',
          cliTool: 'claude',
          runtimeKind: 'cli',
          runtimeId: 'host-1234',
          provider: 'Anthropic',
          model: 'claude',
        }}
      />
    )

    expect(screen.getByText('Claude')).toBeDefined()
    expect(screen.getAllByText('This computer').length).toBeGreaterThan(0)
    expect(screen.queryByText('Anthropic · claude')).toBeNull()
  })

  test('labels text-only model agents by service and work type', () => {
    render(
      <AgentCard
        agent={{
          ...mockAgent,
          id: 'provider-agent',
          cliTool: undefined,
          runtimeKind: 'api',
          provider: 'OpenAI',
          model: 'gpt-4o-mini',
          projectName: undefined,
          workspaceName: undefined,
        }}
      />
    )

    expect(screen.getByText('OpenAI model service')).toBeDefined()
    expect(screen.getByText('Text-only model')).toBeDefined()
    expect(screen.getByText('Choose a starting project')).toBeDefined()
    expect(screen.queryByText('OpenAI · gpt-4o-mini')).toBeNull()
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
