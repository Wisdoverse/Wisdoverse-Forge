import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import {
  AgentTerminalTab,
  liveWorkStatusLabel,
  liveWorkToolLabel,
} from '@app/features/agents/AgentTerminalTab'

const terminalMocks = vi.hoisted(() => ({
  send: vi.fn(),
  subscribe: vi.fn(() => () => {}),
  subscription: null as null | ((raw: unknown) => void),
  status: 'connected',
  fit: vi.fn(),
  write: vi.fn(),
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
    write = terminalMocks.write
  },
}))

beforeEach(() => {
  terminalMocks.status = 'connected'
  terminalMocks.send.mockClear()
  terminalMocks.subscribe.mockClear()
  terminalMocks.subscription = null
  terminalMocks.subscribe.mockImplementation((handler: (raw: unknown) => void) => {
    terminalMocks.subscription = handler
    return () => {}
  })
  terminalMocks.fit.mockClear()
  terminalMocks.write.mockClear()
})

afterEach(() => {
  cleanup()
})

describe('AgentTerminalTab', () => {
  test('labels the virtual keyboard control for beginner operators', () => {
    render(<AgentTerminalTab agentId="agent-1" agentName="Runner" containerId="container-1" />)

    const toggle = screen.getByRole('button', { name: /hide virtual keyboard/i })

    expect(screen.getByText('Ready for live work')).toBeDefined()
    expect(screen.queryByText(/container-1/i)).toBeNull()
    expect(within(toggle).getByText('Keyboard')).toBeDefined()
    expect(screen.getByText('Shortcut keys send to live work')).toBeDefined()
    expect(screen.queryByText(/command window/i)).toBeNull()
    expect(screen.queryByText(/send to terminal/i)).toBeNull()
    expect(screen.getByRole('button', { name: 'Enter' })).toBeEnabled()
  })

  test('keeps the keyboard label visible when shortcut keys are collapsed', () => {
    render(<AgentTerminalTab agentId="agent-1" agentName="Runner" containerId="container-1" />)

    fireEvent.click(screen.getByRole('button', { name: /hide virtual keyboard/i }))

    const toggle = screen.getByRole('button', { name: /show virtual keyboard/i })
    expect(within(toggle).getByText('Keyboard')).toBeDefined()
    expect(screen.queryByText('Shortcut keys send to live work')).toBeNull()
    expect(screen.queryByRole('button', { name: 'Enter' })).toBeNull()
  })

  test('explains that shortcut keys wait for connected live work', () => {
    terminalMocks.status = 'disconnected'

    render(<AgentTerminalTab agentId="agent-1" agentName="Runner" containerId="container-1" />)

    expect(screen.getByText('Wait for live work before using keys')).toBeDefined()
    expect(screen.queryByText(/command window/i)).toBeNull()
    expect(screen.getByRole('button', { name: 'Enter' })).toBeDisabled()
  })

  test('prints a recovery step instead of raw live work connection errors', () => {
    render(<AgentTerminalTab agentId="agent-1" agentName="Runner" containerId="container-1" />)

    act(() => {
      terminalMocks.subscription?.({
        type: 'terminal_error',
        payload: { agentId: 'agent-1', message: 'HTTP 500: pty connection failed' },
      })
    })

    const notice = String(terminalMocks.write.mock.calls.at(-1)?.[0] ?? '')
    expect(notice).toContain('Live work notice: Connection dropped.')
    expect(notice).toContain('Refresh this page first')
    expect(notice).toContain('Controls')
    expect(notice).toContain('Restart agent')
    expect(notice).not.toContain('Command window')
    expect(notice).not.toContain('HTTP 500')
    expect(notice).not.toContain('pty connection failed')
    expect(notice).not.toContain('[terminal]')
  })

  test('shows a beginner unavailable state while live work is starting', () => {
    render(<AgentTerminalTab agentId="agent-1" agentName="Runner" />)

    expect(screen.getByText('Live work is still starting')).toBeInTheDocument()
    expect(
      screen.getByText(
        'Wait until this agent shows Ready. If it stays Offline, open Controls and start or restart this agent before using Live work.'
      )
    ).toBeInTheDocument()
    expect(screen.getByText('Status not reported')).toBeInTheDocument()
    expect(screen.getByText('Agent startup')).toBeInTheDocument()
    expect(screen.getByText('Waiting for this agent')).toBeInTheDocument()
    expect(screen.queryByText(new RegExp(['restart', 'the workspace'].join(' '), 'i'))).toBeNull()
    expect(screen.queryByText(/command window/i)).toBeNull()
    expect(screen.queryByText(/terminal unavailable/i)).toBeNull()
    expect(screen.queryByText(/unknown/i)).toBeNull()
    expect(screen.queryByText('cli')).toBeNull()
  })

  test('labels unavailable live work with readable tool and status names', () => {
    expect(liveWorkToolLabel('codex')).toBe('Codex')
    expect(liveWorkStatusLabel('idle')).toBe('Ready')

    render(
      <AgentTerminalTab agentId="agent-1" agentName="Runner" cliTool="codex" agentStatus="idle" />
    )

    const unavailable = screen.getByTestId('agent-terminal-unavailable')
    expect(within(unavailable).getAllByText('Codex').length).toBeGreaterThan(0)
    expect(within(unavailable).getByText('Ready')).toBeInTheDocument()
    expect(unavailable.textContent).not.toContain('codex')
    expect(unavailable.textContent).not.toContain('idle')
  })
})
