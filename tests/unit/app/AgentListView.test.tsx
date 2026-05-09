import { describe, test, expect, afterEach, beforeEach, vi } from 'vitest'
import { render, screen, cleanup, fireEvent, waitFor } from '@testing-library/react'
import { AgentListView } from '@app/features/agents/AgentListView'
import { useAgentsStore } from '@app/shared/model/agents.store'
import { useBoardStore } from '@app/shared/model/board.store'
import { useNavigationStore } from '@app/entities/navigation'

afterEach(() => {
  cleanup()
  useAgentsStore.getState().reset()
  useBoardStore.getState().reset()
  useNavigationStore.getState().reset()
})

beforeEach(() => {
  useAgentsStore.getState().reset()
  useBoardStore.getState().reset()
  useNavigationStore.getState().reset()
})

describe('AgentListView', () => {
  test('shows empty state when no agents', () => {
    render(<AgentListView />)
    expect(screen.getByText(/no agents/i)).toBeDefined()
  })

  test('renders agent cards', () => {
    useAgentsStore.getState().setAgents([
      {
        id: 'a1',
        name: 'Claude-1',
        provider: 'Anthropic',
        model: 'claude-4-opus',
        status: 'online',
        tasksCompleted: 12,
        tasksInProgress: 1,
        successRate: 0.98,
      },
      {
        id: 'a2',
        name: 'Gemini-1',
        provider: 'Google',
        model: 'gemini-2.5-pro',
        status: 'offline',
        tasksCompleted: 5,
        tasksInProgress: 0,
        successRate: 0.9,
      },
    ])
    render(<AgentListView />)
    expect(screen.getByText('Claude-1')).toBeDefined()
    expect(screen.getByText('Gemini-1')).toBeDefined()
  })

  test('shows agent status indicators', () => {
    useAgentsStore.getState().setAgents([
      {
        id: 'a1',
        name: 'Claude-1',
        provider: 'Anthropic',
        model: 'claude-4-opus',
        status: 'online',
        tasksCompleted: 12,
        tasksInProgress: 1,
        successRate: 0.98,
      },
    ])
    render(<AgentListView />)
    expect(screen.getByTestId('agent-status-a1')).toBeDefined()
  })

  test('shows + New Agent button', () => {
    render(<AgentListView />)
    // Both the toolbar and the empty-state CTA render "New Agent"
    expect(screen.getAllByText(/new agent/i).length).toBeGreaterThan(0)
  })

  test('creates a task group from the selected project context', async () => {
    const createAgentGroup = vi.fn(
      async (projectId: string, input: { name: string; description?: string }) => {
        const group = { id: 'g-new', name: input.name, projectId }
        useNavigationStore.setState({ agentGroups: [group] })
        useBoardStore.getState().setSelectedGroupId(group.id)
        return group
      }
    )
    useNavigationStore.setState({
      selectedProjectId: 'p1',
      projects: {
        t1: [
          {
            id: 'p1',
            teamId: 't1',
            name: 'Platform',
            slug: 'platform',
            color: '#007AFF',
            description: '',
          },
        ],
      },
      agentGroups: [],
      createAgentGroup,
    } as never)

    render(<AgentListView />)

    expect(screen.getByText('Task Routing')).toBeDefined()
    fireEvent.change(screen.getByLabelText(/task group name/i), {
      target: { value: 'Frontend Delivery' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^create$/i }))

    await waitFor(() =>
      expect(createAgentGroup).toHaveBeenCalledWith(
        'p1',
        expect.objectContaining({
          name: 'Frontend Delivery',
          description: 'Agents in this group can receive tasks from the board.',
        })
      )
    )
    expect(useBoardStore.getState().selectedGroupId).toBe('g-new')
    expect(screen.getByRole('button', { name: /frontend delivery/i })).toHaveAttribute(
      'aria-pressed',
      'true'
    )
  })
})
