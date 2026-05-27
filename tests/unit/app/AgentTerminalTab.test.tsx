import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { AgentTerminalTab } from '@app/features/agents/AgentTerminalTab'

const terminalMocks = vi.hoisted(() => ({
  send: vi.fn(),
  subscribe: vi.fn(() => () => {}),
  status: 'connected',
  fit: vi.fn(),
}))

vi.mock('@app/shared/model/websocket.context', () => ({
  useWebSocket: () => ({
    status: terminalMocks.status,
    send: terminalMocks.send,
    subscribe: terminalMocks.subscribe,
  }),
}))

vi.mock('@xterm/addon-fit', () => ({
  FitAddon: class FitAddon {
    fit = terminalMocks.fit
  },
}))

vi.mock('@xterm/xterm', () => ({
  Terminal: class Terminal {
    cols = 80
    rows = 24
    dispose = vi.fn()
    loadAddon = vi.fn()
    onData = vi.fn()
    onResize = vi.fn()
    open = vi.fn()
    reset = vi.fn()
    write = vi.fn()
  },
}))

beforeEach(() => {
  terminalMocks.status = 'connected'
  terminalMocks.send.mockClear()
  terminalMocks.subscribe.mockClear()
  terminalMocks.fit.mockClear()
})

afterEach(() => {
  cleanup()
})

describe('AgentTerminalTab', () => {
  test('labels the virtual keyboard control for beginner operators', () => {
    render(<AgentTerminalTab agentId="agent-1" agentName="Runner" containerId="container-1" />)

    const toggle = screen.getByRole('button', { name: /hide virtual keyboard/i })

    expect(within(toggle).getByText('Keyboard')).toBeDefined()
    expect(screen.getByText('Shortcut keys send to terminal')).toBeDefined()
    expect(screen.getByRole('button', { name: 'Enter' })).toBeEnabled()
  })

  test('keeps the keyboard label visible when shortcut keys are collapsed', () => {
    render(<AgentTerminalTab agentId="agent-1" agentName="Runner" containerId="container-1" />)

    fireEvent.click(screen.getByRole('button', { name: /hide virtual keyboard/i }))

    const toggle = screen.getByRole('button', { name: /show virtual keyboard/i })
    expect(within(toggle).getByText('Keyboard')).toBeDefined()
    expect(screen.queryByText('Shortcut keys send to terminal')).toBeNull()
    expect(screen.queryByRole('button', { name: 'Enter' })).toBeNull()
  })

  test('explains that shortcut keys wait for a connected terminal', () => {
    terminalMocks.status = 'disconnected'

    render(<AgentTerminalTab agentId="agent-1" agentName="Runner" containerId="container-1" />)

    expect(screen.getByText('Connect terminal to use keys')).toBeDefined()
    expect(screen.getByRole('button', { name: 'Enter' })).toBeDisabled()
  })
})
