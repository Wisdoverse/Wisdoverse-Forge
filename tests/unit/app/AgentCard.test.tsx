import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { agentCardStatusHelp, AgentCard } from '@app/features/agents/AgentCard'
import { agentStatusLabel, type AgentInfo } from '@app/entities/agent'

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
  test('explains when a ready agent can take work', () => {
    render(<AgentCard agent={mockAgent} />)

    expect(screen.getByTestId('agent-status-agent-1').textContent).toContain('Ready')
    expect(screen.getByTestId('agent-status-help-agent-1').textContent).toBe('Ready for a new task')
    expect(screen.getByText('Finished')).toBeDefined()
    expect(screen.getByText('Running')).toBeDefined()
    expect(screen.getByText('Success')).toBeDefined()
  })

  test('uses action-oriented status labels', () => {
    expect(agentStatusLabel('working')).toBe('Working now')
    expect(agentStatusLabel('idle')).toBe('Ready')
    expect(agentStatusLabel('offline')).toBe('Not connected')
    expect(agentStatusLabel(null)).toBe('Refresh agent status')
    expect(agentStatusLabel('future_status')).toBe('Check agent status')
    expect(agentStatusLabel('offline')).not.toBe('Offline')
    expect(agentStatusLabel(null)).not.toBe('Status not reported')
    expect(agentStatusLabel('future_status')).not.toBe('Status needs review')
  })

  test('summarizes managed workspace agents without raw provider/model pairs', () => {
    render(<AgentCard agent={mockAgent} />)

    expect(screen.getByText('Codex')).toBeDefined()
    expect(screen.getAllByText('Managed workspace').length).toBeGreaterThan(0)
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

  test('labels chat-only agents by service and work type', () => {
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

    expect(screen.getByText('OpenAI AI service')).toBeDefined()
    expect(screen.getByText('Chat-only AI service')).toBeDefined()
    expect(screen.getByText('Open project settings first.')).toBeDefined()
    expect(screen.queryByText('Choose a project from the sidebar first.')).toBeNull()
    expect(screen.queryByText('Choose a starting project')).toBeNull()
    expect(screen.queryByText('Chat-only agent')).toBeNull()
    expect(screen.queryByText('OpenAI · gpt-4o-mini')).toBeNull()
    expect(screen.queryByText(/model service/i)).toBeNull()
    expect(screen.queryByText(/text-only model/i)).toBeNull()
  })

  test('keeps review-needed AI service labels readable on chat-only cards', () => {
    render(
      <AgentCard
        agent={{
          ...mockAgent,
          id: 'review-provider',
          cliTool: undefined,
          runtimeKind: 'api',
          provider: 'Check AI service',
          model: 'Model not reported',
        }}
      />
    )

    expect(screen.getByText('Check AI service')).toBeDefined()
    expect(screen.queryByText('Check AI service AI service')).toBeNull()
  })

  test('warns before assigning work to an offline agent', () => {
    expect(agentCardStatusHelp('offline')).toBe('Open this agent to reconnect before work')

    render(<AgentCard agent={{ ...mockAgent, status: 'offline' }} />)

    expect(screen.getByTestId('agent-status-help-agent-1').textContent).toBe(
      'Open this agent to reconnect before work'
    )
  })

  test('labels unknown agent statuses without exposing backend values', () => {
    expect(agentCardStatusHelp('warming_up')).toBe('Check this agent before sending work')

    render(<AgentCard agent={{ ...mockAgent, status: 'warming_up' as never }} />)

    expect(screen.getByTestId('agent-status-agent-1').textContent).toContain('Check agent status')
    expect(screen.getByTestId('agent-status-help-agent-1').textContent).toBe(
      'Check this agent before sending work'
    )
    expect(screen.queryByText(/warming_up/i)).toBeNull()
    expect(screen.queryByText(/warming up/i)).toBeNull()
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
