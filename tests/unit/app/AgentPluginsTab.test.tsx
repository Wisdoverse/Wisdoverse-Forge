import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { AgentPluginsTab, pluginSettingNote } from '@app/features/agents/AgentPluginsTab'

const fetchMock = vi.fn()

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

beforeEach(() => {
  fetchMock.mockReset()
  vi.stubGlobal('fetch', fetchMock)
  localStorage.clear()
})

function pluginResponse() {
  return {
    ok: true,
    json: async () => ({
      ok: true,
      plugins: [
        {
          pluginId: 'shell',
          name: 'Shell Tools',
          version: '1.0.0',
          description: 'Run command workflows',
          pluginEnabled: true,
          enabled: null,
        },
        {
          pluginId: 'browser',
          name: 'Browser Tools',
          version: '1.1.0',
          description: 'Inspect browser surfaces',
          pluginEnabled: false,
          enabled: true,
        },
        {
          pluginId: 'deploy',
          name: 'Deploy Tools',
          version: '2.0.0',
          description: 'Release services',
          pluginEnabled: true,
          enabled: false,
        },
      ],
    }),
  }
}

describe('AgentPluginsTab', () => {
  test('explains tool loading for first-time agent setup', () => {
    fetchMock.mockImplementationOnce(() => new Promise(() => undefined))

    render(<AgentPluginsTab agentId="agent-1" />)

    const loading = screen.getByRole('status', { name: /checking this agent's tools/i })
    expect(loading).toHaveTextContent("Checking this agent's tools")
    expect(loading).toHaveTextContent(
      'Forge is checking which tools this agent can use for its next task.'
    )
    expect(loading).toHaveTextContent(
      'If this takes more than a moment, open Tools again or ask an owner or admin to check tool access.'
    )
    expect(loading).toHaveTextContent(
      'Success looks like available tools or an ask-an-owner step.'
    )
    expect(loading).not.toHaveTextContent("Loading this agent's tools")
  })

  test('summarizes plugin readiness for an agent', async () => {
    fetchMock.mockResolvedValueOnce(pluginResponse())

    render(<AgentPluginsTab agentId="agent-1" />)

    const readiness = await screen.findByTestId('agent-plugin-readiness')
    expect(readiness).toBeDefined()
    expect(within(readiness).getByText('What this agent can use')).toBeDefined()
    expect(
      within(readiness).getByText(
        'Tools are extra abilities. Only turn on tools this agent needs for its next tasks. If you are not sure, keep the team setting and ask an owner before changing access.'
      )
    ).toBeDefined()
    expect(
      within(readiness).getByText("Saved changes apply to this agent's next task.")
    ).toBeDefined()
    expect(screen.getByRole('group', { name: /tool filter/i })).toBeDefined()
    expect(
      within(screen.getByTestId('agent-plugin-metric-enabled')).getByText('Can use now')
    ).toBeDefined()
    expect(within(screen.getByTestId('agent-plugin-metric-enabled')).getByText('2')).toBeDefined()
    expect(within(screen.getByTestId('agent-plugin-metric-disabled')).getByText('1')).toBeDefined()
    expect(within(screen.getByTestId('agent-plugin-metric-overrides')).getByText('2')).toBeDefined()
    expect(within(screen.getByTestId('agent-plugin-metric-total')).getByText('3')).toBeDefined()
    expect(screen.getByText('Shell Tools')).toBeDefined()
    expect(screen.getByText('Browser Tools')).toBeDefined()
    expect(screen.getByText('Deploy Tools')).toBeDefined()
    expect(screen.getAllByText('Can use now').length).toBeGreaterThan(0)
    expect(screen.getByText('Turned off for this agent')).toBeDefined()
    expect(screen.queryByText('Agent can use')).toBeNull()
    expect(screen.queryByText('Not available')).toBeNull()
    expect(screen.getByText('Using team setting - normally available for agents')).toBeDefined()
    expect(screen.getByText('Changed for this agent - normally off for agents')).toBeDefined()
    expect(screen.getByLabelText("Search this agent's tools")).toHaveAccessibleDescription(
      "Search only filters this agent's tools. Use Show all tools to return to the full list."
    )
    expect(screen.queryByText(new RegExp(['workspace', 'default'].join(' '), 'i'))).toBeNull()
    expect(screen.queryByText(new RegExp(['workspace', 'setting'].join(' '), 'i'))).toBeNull()
    expect(screen.queryByRole('group', { name: /plugin filter/i })).toBeNull()
  })

  test('guides first-time users when no tools are available for an agent', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: async () => ({ ok: true, plugins: [] }),
    })

    render(<AgentPluginsTab agentId="agent-1" />)

    const empty = await screen.findByTestId('agent-plugin-empty')
    expect(within(empty).getByText('Ask an owner or admin to add tools')).toBeDefined()
    expect(within(empty).getByText('Open Settings.')).toBeDefined()
    expect(
      within(empty).getByText('Ask an owner or admin to add one tool for this team.')
    ).toBeDefined()
    expect(within(empty).getByText('Come back here after tools are added.')).toBeDefined()
    expect(empty.textContent).toContain(
      'Success looks like a tool listed with Can use now or Turned off for this agent.'
    )
    expect(empty.textContent).not.toContain('plugin')
    expect(empty.textContent).not.toContain('workspace')
    expect(empty.textContent).not.toContain('configuration')
  })

  test('explains per-agent tool settings without raw on and off jargon', () => {
    expect(pluginSettingNote({ defaultEnabled: true, hasOverride: false })).toBe(
      'Using team setting - normally available for agents'
    )
    expect(pluginSettingNote({ defaultEnabled: false, hasOverride: true })).toBe(
      'Changed for this agent - normally off for agents'
    )
  })

  test('filters and searches agent plugins', async () => {
    fetchMock.mockResolvedValueOnce(pluginResponse())

    render(<AgentPluginsTab agentId="agent-1" />)

    await screen.findByText('Shell Tools')
    const filters = screen.getByTestId('agent-plugin-filter')
    const turnedOffFilter = within(filters).getByRole('button', {
      name: /show tools turned off for this agent, 1 matching tool/i,
    })
    expect(
      within(filters).getByRole('button', {
        name: /show all tools for this agent, 3 matching tools/i,
      })
    ).toHaveAttribute('aria-pressed', 'true')
    fireEvent.click(turnedOffFilter)
    expect(turnedOffFilter).toHaveAttribute('aria-pressed', 'true')

    expect(screen.getByText('Deploy Tools')).toBeDefined()
    expect(screen.queryByText('Shell Tools')).toBeNull()
    expect(screen.queryByText('Browser Tools')).toBeNull()

    fireEvent.change(screen.getByTestId('agent-plugin-search'), { target: { value: 'browser' } })
    const combinedEmpty = screen.getByTestId('agent-plugin-filter-empty')
    expect(combinedEmpty).toHaveAttribute('role', 'status')
    expect(combinedEmpty).toHaveAttribute('aria-live', 'polite')
    expect(within(combinedEmpty).getByText('Search and filter are hiding tools')).toBeDefined()
    expect(combinedEmpty.textContent).toContain(
      'Use Show all tools before assuming this agent has no matching tool.'
    )
    expect(combinedEmpty.textContent).not.toContain('No tools match this view')

    fireEvent.click(within(combinedEmpty).getByRole('button', { name: /show all tools/i }))
    expect(screen.getByText('Shell Tools')).toBeDefined()
    expect(screen.getByText('Browser Tools')).toBeDefined()
    expect(screen.getByText('Deploy Tools')).toBeDefined()
    expect(screen.getByLabelText("Search this agent's tools")).toHaveValue('')
    expect(
      within(filters).getByRole('button', {
        name: /show all tools for this agent, 3 matching tools/i,
      })
    ).toHaveAttribute('aria-pressed', 'true')
  })

  test('searches only visible tool names and summaries', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        ok: true,
        plugins: [
          {
            pluginId: 'internal-command-runner',
            name: 'Command Runner',
            version: '2026.06-internal',
            description: 'Run approved command workflows',
            pluginEnabled: true,
            enabled: null,
          },
        ],
      }),
    })

    render(<AgentPluginsTab agentId="agent-1" />)

    await screen.findByText('Command Runner')
    fireEvent.change(screen.getByTestId('agent-plugin-search'), {
      target: { value: '2026.06-internal' },
    })

    const empty = screen.getByTestId('agent-plugin-filter-empty')
    expect(within(empty).getByText('Search is hiding tools')).toBeDefined()
    expect(screen.queryByText('Command Runner')).toBeNull()

    fireEvent.click(within(empty).getByRole('button', { name: /show all tools/i }))
    fireEvent.change(screen.getByTestId('agent-plugin-search'), {
      target: { value: 'command workflows' },
    })

    expect(screen.getByText('Command Runner')).toBeDefined()
  })

  test('explains search-only empty tool lists', async () => {
    fetchMock.mockResolvedValueOnce(pluginResponse())

    render(<AgentPluginsTab agentId="agent-1" />)

    await screen.findByText('Shell Tools')

    fireEvent.change(screen.getByTestId('agent-plugin-search'), { target: { value: 'missing' } })
    const searchEmpty = screen.getByTestId('agent-plugin-filter-empty')
    expect(searchEmpty).toHaveAttribute('role', 'status')
    expect(searchEmpty).toHaveAttribute('aria-live', 'polite')
    expect(within(searchEmpty).getByText('Search is hiding tools')).toBeDefined()
    expect(searchEmpty.textContent).toContain('Use Show all tools to return to the full list.')
    expect(searchEmpty.textContent).not.toContain('No tools match this view')

    fireEvent.click(within(searchEmpty).getByRole('button', { name: /show all tools/i }))
    expect(screen.getByText('Shell Tools')).toBeDefined()
    expect(screen.getByLabelText("Search this agent's tools")).toHaveValue('')
  })

  test('explains filter-only empty tool lists', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        ok: true,
        plugins: [
          {
            pluginId: 'shell',
            name: 'Shell Tools',
            version: '1.0.0',
            description: 'Run command workflows',
            pluginEnabled: true,
            enabled: null,
          },
        ],
      }),
    })

    render(<AgentPluginsTab agentId="agent-1" />)

    await screen.findByText('Shell Tools')
    const filters = screen.getByTestId('agent-plugin-filter')
    fireEvent.click(
      within(filters).getByRole('button', {
        name: /show tools turned off for this agent, 0 matching tools/i,
      })
    )

    const filterEmpty = screen.getByTestId('agent-plugin-filter-empty')
    expect(filterEmpty).toHaveAttribute('role', 'status')
    expect(filterEmpty).toHaveAttribute('aria-live', 'polite')
    expect(within(filterEmpty).getByText('Filter is hiding tools')).toBeDefined()
    expect(filterEmpty.textContent).toContain('Use Show all tools to return to the full list.')
    expect(filterEmpty.textContent).not.toContain('No tools match this view')

    fireEvent.click(within(filterEmpty).getByRole('button', { name: /show all tools/i }))

    expect(screen.getByText('Shell Tools')).toBeDefined()
    expect(
      within(filters).getByRole('button', {
        name: /show all tools for this agent, 1 matching tool/i,
      })
    ).toHaveAttribute('aria-pressed', 'true')
  })

  test('guides users when a tool has no summary yet', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        ok: true,
        plugins: [
          {
            pluginId: 'unknown',
            name: 'Workspace Helper',
            version: '1.0.0',
            description: '',
            pluginEnabled: false,
            enabled: null,
          },
        ],
      }),
    })

    render(<AgentPluginsTab agentId="agent-1" />)

    expect(await screen.findByText('Workspace Helper')).toBeDefined()
    expect(
      screen.getByText(
        'Tool summary is missing. Keep the team setting until an owner explains what this tool lets the agent do.'
      )
    ).toBeDefined()
    expect(screen.queryByText(/before turning it on/i)).toBeNull()
    expect(screen.queryByText(/No tool summary yet/i)).toBeNull()
    expect(screen.queryByText('No description provided')).toBeNull()
  })

  test('toggles a plugin with an agent-level override', async () => {
    fetchMock.mockResolvedValueOnce(pluginResponse()).mockResolvedValueOnce({
      ok: true,
      json: async () => ({ ok: true }),
    })

    render(<AgentPluginsTab agentId="agent-1" />)

    const shellSwitch = await screen.findByRole('switch', {
      name: /turn off shell tools for this agent/i,
    })
    expect(shellSwitch).toHaveAttribute('aria-checked', 'true')

    fireEvent.click(shellSwitch)

    await waitFor(() => {
      expect(fetchMock).toHaveBeenLastCalledWith(
        '/api/v1/agents/agent-1/plugins/shell',
        expect.objectContaining({
          method: 'PUT',
          body: JSON.stringify({ enabled: false }),
        })
      )
    })
    expect(shellSwitch).toHaveAttribute('aria-checked', 'false')
  })

  test('shows beginner recovery steps when tools cannot be loaded', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: false,
      status: 403,
    })
    const onBackToAgents = vi.fn()

    render(<AgentPluginsTab agentId="agent-1" onBackToAgents={onBackToAgents} />)

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(within(alert).getByText('Open Tools again from Agents')).toBeDefined()
    fireEvent.click(within(alert).getByRole('button', { name: /back to agents/i }))
    expect(onBackToAgents).toHaveBeenCalledTimes(1)
    expect(alert.textContent?.match(/Go back to Agents, choose this agent again/g)).toHaveLength(1)
    expect(alert.textContent).toContain(
      "Ask an owner or admin to give you access to this agent's tools."
    )
    expect(alert.textContent).not.toContain('Agent tools need attention.')
    expect(alert.textContent).not.toContain('HTTP 403')
    expect(alert.textContent).not.toContain('Details:')
  })

  test('explains failed tool changes and restores the previous switch state', async () => {
    fetchMock.mockResolvedValueOnce(pluginResponse()).mockResolvedValueOnce({
      ok: false,
      status: 403,
    })

    render(<AgentPluginsTab agentId="agent-1" />)

    const shellSwitch = await screen.findByRole('switch', {
      name: /turn off shell tools for this agent/i,
    })
    expect(shellSwitch).toHaveAttribute('aria-checked', 'true')

    fireEvent.click(shellSwitch)

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert.textContent).toContain('The switch was returned to its previous setting.')
    expect(alert.textContent).toContain(
      "Ask an owner or admin to give you access to this agent's tools."
    )
    expect(alert.textContent).not.toContain('HTTP 403')
    expect(shellSwitch).toHaveAttribute('aria-checked', 'true')
  })

  test('scrolls the tool change error into view again when the same save failure repeats', async () => {
    const scrollSpy = vi
      .spyOn(Element.prototype, 'scrollIntoView')
      .mockImplementation(() => undefined)
    fetchMock.mockResolvedValueOnce(pluginResponse()).mockResolvedValue({
      ok: false,
      status: 403,
    })

    render(<AgentPluginsTab agentId="agent-1" />)

    const shellSwitch = await screen.findByRole('switch', {
      name: /turn off shell tools for this agent/i,
    })

    fireEvent.click(shellSwitch)
    const alert = await screen.findByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert.textContent).toContain('The switch was returned to its previous setting.')
    await waitFor(() => expect(scrollSpy.mock.calls.length).toBeGreaterThan(0))
    const callsAfterFirstFailure = scrollSpy.mock.calls.length

    fireEvent.click(shellSwitch)
    await waitFor(() => expect(scrollSpy.mock.calls.length).toBeGreaterThan(callsAfterFirstFailure))
    scrollSpy.mockRestore()
  })

  test('shows beginner next steps when no tools are available', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: async () => ({ ok: true, plugins: [] }),
    })

    render(<AgentPluginsTab agentId="agent-1" />)

    const emptyState = await screen.findByTestId('agent-plugin-empty')
    expect(within(emptyState).getByText('Ask an owner or admin to add tools')).toBeDefined()
    expect(emptyState.textContent).toContain(
      'Tools give agents extra abilities. After tools are added, return here to choose which ones this agent can use.'
    )
    expect(emptyState.textContent).not.toContain('No tools are available for this agent yet')
    expect(screen.queryByText(new RegExp(['assign', 'ing them here'].join(''), 'i'))).toBeNull()
  })
})
