import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { AgentPluginsTab } from '@app/features/agents/AgentPluginsTab'

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
    expect(
      within(screen.getByTestId('agent-plugin-metric-enabled')).getByText('Enabled')
    ).toBeDefined()
    expect(within(screen.getByTestId('agent-plugin-metric-enabled')).getByText('2')).toBeDefined()
    expect(within(screen.getByTestId('agent-plugin-metric-disabled')).getByText('1')).toBeDefined()
    expect(within(screen.getByTestId('agent-plugin-metric-overrides')).getByText('2')).toBeDefined()
    expect(within(screen.getByTestId('agent-plugin-metric-total')).getByText('3')).toBeDefined()
    expect(screen.getByText('Shell Tools')).toBeDefined()
    expect(screen.getByText('Browser Tools')).toBeDefined()
    expect(screen.getByText('Deploy Tools')).toBeDefined()
  })

  test('filters and searches agent plugins', async () => {
    fetchMock.mockResolvedValueOnce(pluginResponse())

    render(<AgentPluginsTab agentId="agent-1" />)

    await screen.findByText('Shell Tools')
    const filters = screen.getByTestId('agent-plugin-filter')
    fireEvent.click(within(filters).getByRole('button', { name: /disabled\s*1/i }))

    expect(screen.getByText('Deploy Tools')).toBeDefined()
    expect(screen.queryByText('Shell Tools')).toBeNull()
    expect(screen.queryByText('Browser Tools')).toBeNull()

    fireEvent.change(screen.getByTestId('agent-plugin-search'), { target: { value: 'browser' } })
    expect(screen.getByTestId('agent-plugin-filter-empty')).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /clear filters/i }))
    expect(screen.getByText('Shell Tools')).toBeDefined()
    expect(screen.getByText('Browser Tools')).toBeDefined()
    expect(screen.getByText('Deploy Tools')).toBeDefined()
  })

  test('toggles a plugin with an agent-level override', async () => {
    fetchMock.mockResolvedValueOnce(pluginResponse()).mockResolvedValueOnce({
      ok: true,
      json: async () => ({ ok: true }),
    })

    render(<AgentPluginsTab agentId="agent-1" />)

    const shellSwitch = await screen.findByRole('switch', { name: /toggle shell tools/i })
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
})
