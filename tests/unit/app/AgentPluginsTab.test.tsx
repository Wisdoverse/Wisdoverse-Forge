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
  test('summarizes plugin readiness for an agent', async () => {
    fetchMock.mockResolvedValueOnce(pluginResponse())

    render(<AgentPluginsTab agentId="agent-1" />)

    const readiness = await screen.findByTestId('agent-plugin-readiness')
    expect(readiness).toBeDefined()
    expect(within(readiness).getByText('What this agent can use')).toBeDefined()
    expect(
      within(readiness).getByText(
        'Tools are extra abilities. Turning one on or off here affects only this agent.'
      )
    ).toBeDefined()
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
    expect(screen.getByText('Using team setting - normally available for agents')).toBeDefined()
    expect(screen.getByText('Changed for this agent - normally off for agents')).toBeDefined()
    expect(screen.queryByText(new RegExp(['workspace', 'default'].join(' '), 'i'))).toBeNull()
    expect(screen.queryByText(new RegExp(['workspace', 'setting'].join(' '), 'i'))).toBeNull()
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
    fireEvent.click(within(filters).getByRole('button', { name: /turned off\s*1/i }))

    expect(screen.getByText('Deploy Tools')).toBeDefined()
    expect(screen.queryByText('Shell Tools')).toBeNull()
    expect(screen.queryByText('Browser Tools')).toBeNull()

    fireEvent.change(screen.getByTestId('agent-plugin-search'), { target: { value: 'browser' } })
    const combinedEmpty = screen.getByTestId('agent-plugin-filter-empty')
    expect(within(combinedEmpty).getByText('Clear search or show all tools')).toBeDefined()
    expect(combinedEmpty.textContent).toContain(
      'This agent has tools, but the current search and filter hide them.'
    )
    expect(combinedEmpty.textContent).not.toContain('No tools match this view')

    fireEvent.click(screen.getByRole('button', { name: /show all tools/i }))
    expect(screen.getByText('Shell Tools')).toBeDefined()
    expect(screen.getByText('Browser Tools')).toBeDefined()
    expect(screen.getByText('Deploy Tools')).toBeDefined()
  })

  test('explains search-only empty tool lists', async () => {
    fetchMock.mockResolvedValueOnce(pluginResponse())

    render(<AgentPluginsTab agentId="agent-1" />)

    await screen.findByText('Shell Tools')

    fireEvent.change(screen.getByTestId('agent-plugin-search'), { target: { value: 'missing' } })
    const searchEmpty = screen.getByTestId('agent-plugin-filter-empty')
    expect(within(searchEmpty).getByText('Clear search to see tools')).toBeDefined()
    expect(searchEmpty.textContent).toContain(
      'This agent has tools, but the search hides them. Try a broader word or clear search.'
    )
    expect(searchEmpty.textContent).not.toContain('No tools match this view')

    fireEvent.click(screen.getByRole('button', { name: /show all tools/i }))
    expect(screen.getByText('Shell Tools')).toBeDefined()
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
    fireEvent.click(within(filters).getByRole('button', { name: /turned off\s*0/i }))

    const filterEmpty = screen.getByTestId('agent-plugin-filter-empty')
    expect(within(filterEmpty).getByText('Choose All to see tools')).toBeDefined()
    expect(filterEmpty.textContent).toContain(
      'This agent has tools, but this filter has no results yet.'
    )
    expect(filterEmpty.textContent).not.toContain('No tools match this view')
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

    render(<AgentPluginsTab agentId="agent-1" />)

    const alert = await screen.findByRole('alert')
    expect(within(alert).getByText('Agent tools need attention.')).toBeDefined()
    expect(alert.textContent).toContain(
      "Ask an owner or admin to give you access to this agent's tools."
    )
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
    expect(alert.textContent).toContain(
      'Tool change was not saved. The switch was returned to its previous setting.'
    )
    expect(alert.textContent).toContain(
      "Ask an owner or admin to give you access to this agent's tools."
    )
    expect(alert.textContent).not.toContain('HTTP 403')
    expect(shellSwitch).toHaveAttribute('aria-checked', 'true')
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
