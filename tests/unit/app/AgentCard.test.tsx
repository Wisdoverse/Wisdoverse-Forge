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
  test('uses the status dot without a duplicate ready banner', () => {
    render(<AgentCard agent={mockAgent} />)

    expect(screen.getByTestId('agent-status-agent-1').textContent).toContain('Ready')
    expect(screen.queryByTestId('agent-status-help-agent-1')).toBeNull()
    expect(screen.getByText('Finished')).toBeDefined()
    expect(screen.getByText('Running')).toBeDefined()
    expect(screen.getByText('Success')).toBeDefined()
  })

  test('hides an all-zero metric group', () => {
    render(
      <AgentCard
        agent={{
          ...mockAgent,
          tasksCompleted: 0,
          tasksInProgress: 0,
          successRate: 0,
        }}
      />
    )

    expect(screen.queryByText('Finished')).toBeNull()
    expect(screen.queryByText('Running')).toBeNull()
    expect(screen.queryByText('Success')).toBeNull()
  })

  test('uses action-oriented status labels', () => {
    expect(agentStatusLabel('working')).toBe('Working now')
    expect(agentStatusLabel('idle')).toBe('Ready')
    expect(agentStatusLabel('offline')).toBe('Not connected')
    expect(agentStatusLabel(null)).toBe('Check if ready')
    expect(agentStatusLabel('future_status')).toBe('Check if ready')
    expect(agentStatusLabel('offline')).not.toBe('Offline')
    expect(agentStatusLabel(null)).not.toBe('Status not reported')
    expect(agentStatusLabel('future_status')).not.toBe('Status needs review')
  })

  test('summarizes project-file agents without raw provider/model pairs', () => {
    render(<AgentCard agent={mockAgent} />)

    expect(screen.getByText('Codex')).toBeDefined()
    expect(screen.getAllByText('Project files').length).toBeGreaterThan(0)
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
    expect(screen.getByText('Simple chat agent')).toBeDefined()
    expect(screen.getByText('No project files needed')).toBeDefined()
    expect(screen.getByText('Answered')).toBeDefined()
    expect(screen.getByText('Replying')).toBeDefined()
    expect(screen.getByText('Answer success')).toBeDefined()
    expect(screen.getByTestId('agent-status-help-provider-agent').textContent).toBe(
      'Ready for direct chat. Use an agent with Project files or This computer for Tasks and code changes.'
    )
    expect(screen.queryByText('Ready for a message')).toBeNull()
    expect(screen.queryByText('Open project settings first.')).toBeNull()
    expect(screen.queryByText('Choose a project from the sidebar first.')).toBeNull()
    expect(screen.queryByText('Choose a starting project')).toBeNull()
    expect(screen.queryByText('Ready for a new task')).toBeNull()
    expect(screen.queryByText('Finished')).toBeNull()
    expect(screen.queryByText('Running')).toBeNull()
    expect(screen.queryByText('Chat-only AI service')).toBeNull()
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
      'Open this agent and start project files before sending Tasks or code changes.'
    )
    expect(screen.queryByText(/file work/i)).toBeNull()
    expect(screen.queryByText(/start it/i)).toBeNull()
  })

  test('explains how to reconnect a not-connected agent on this computer', () => {
    const localAgent: AgentInfo = {
      ...mockAgent,
      id: 'local-agent',
      cliTool: 'codex',
      runtimeKind: 'cli',
      runtimeId: 'host-1234',
      status: 'offline',
    }

    expect(agentCardStatusHelp('offline', localAgent)).toBe(
      'Open this agent to see the reconnect steps from Agents.'
    )

    render(<AgentCard agent={localAgent} />)

    expect(screen.getByTestId('agent-status-help-local-agent').textContent).toBe(
      'Open this agent to see the reconnect steps from Agents.'
    )
    expect(screen.queryByText(/Terminal or PowerShell/i)).toBeNull()
    expect(screen.queryByText(/command app/i)).toBeNull()
    expect(screen.queryByText('Open this agent to reconnect before work')).toBeNull()
  })

  test('explains not-connected chat-only agents without file-work guidance', () => {
    const providerAgent: AgentInfo = {
      ...mockAgent,
      id: 'provider-agent',
      cliTool: undefined,
      runtimeKind: 'api',
      status: 'offline',
    }

    expect(agentCardStatusHelp('offline', providerAgent)).toBe(
      'Open this agent and check its AI service before sending a message.'
    )

    render(<AgentCard agent={providerAgent} />)

    expect(screen.getByTestId('agent-status-help-provider-agent').textContent).toBe(
      'Open this agent and check its AI service before sending a message.'
    )
    expect(screen.queryByText(/chat work/i)).toBeNull()
    expect(screen.queryByText(/file work/i)).toBeNull()
  })

  test('describes working chat-only agents as answering messages', () => {
    const providerAgent: AgentInfo = {
      ...mockAgent,
      id: 'provider-agent-working',
      cliTool: undefined,
      runtimeKind: 'api',
      status: 'working',
    }

    expect(agentCardStatusHelp('idle', providerAgent)).toBe(
      'Ready for direct chat. Use an agent with Project files or This computer for Tasks and code changes.'
    )
    expect(agentCardStatusHelp('working', providerAgent)).toBe('Answering a message now')

    render(<AgentCard agent={providerAgent} />)

    expect(screen.queryByTestId('agent-status-help-provider-agent-working')).toBeNull()
    expect(screen.queryByText('Running a task now')).toBeNull()
  })

  test('labels unknown agent statuses without exposing backend values', () => {
    expect(agentCardStatusHelp('warming_up')).toBe('Check this agent before sending work')

    render(<AgentCard agent={{ ...mockAgent, status: 'warming_up' as never }} />)

    expect(screen.getByTestId('agent-status-agent-1').textContent).toContain('Check if ready')
    expect(screen.getByTestId('agent-status-help-agent-1').textContent).toBe(
      'Check this agent before sending work'
    )
    expect(screen.queryByText(/warming_up/i)).toBeNull()
    expect(screen.queryByText(/warming up/i)).toBeNull()
  })

  test('uses message wording for unknown chat-only status', () => {
    const providerAgent: AgentInfo = {
      ...mockAgent,
      id: 'provider-agent-unknown',
      cliTool: undefined,
      runtimeKind: 'api',
      status: 'warming_up' as never,
    }

    expect(agentCardStatusHelp('warming_up', providerAgent)).toBe(
      'Check this agent before sending a message'
    )

    render(<AgentCard agent={providerAgent} />)

    expect(screen.getByTestId('agent-status-help-provider-agent-unknown').textContent).toBe(
      'Check this agent before sending a message'
    )
    expect(screen.queryByText('Check this agent before sending work')).toBeNull()
  })

  test('keeps current task visible without a duplicate working banner', () => {
    render(
      <AgentCard
        agent={{ ...mockAgent, status: 'working', tasksInProgress: 1, currentTask: 'Run tests' }}
      />
    )

    expect(screen.getByText('Run tests')).toBeDefined()
    expect(screen.queryByTestId('agent-status-help-agent-1')).toBeNull()
  })

  test('hides stale task text on working chat-only cards', () => {
    render(
      <AgentCard
        agent={{
          ...mockAgent,
          id: 'provider-agent-working',
          cliTool: undefined,
          runtimeKind: 'api',
          status: 'working',
          tasksInProgress: 1,
          currentTask: 'Run tests',
        }}
      />
    )

    expect(screen.getByText('Answering in Chat')).toBeDefined()
    expect(screen.getByText('Shown in Console')).toBeDefined()
    expect(screen.queryByText('Run tests')).toBeNull()
    expect(screen.queryByText('Console')).toBeNull()
    expect(screen.queryByTestId('agent-status-help-provider-agent-working')).toBeNull()
  })

  test('opens the agent detail when clicked', () => {
    const onClick = vi.fn()
    render(<AgentCard agent={mockAgent} onClick={onClick} />)

    fireEvent.click(screen.getByTestId('agent-card-agent-1'))

    expect(onClick).toHaveBeenCalledOnce()
  })
})
