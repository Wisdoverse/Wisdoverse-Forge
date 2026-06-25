import { describe, test, expect, afterEach, vi, beforeEach } from 'vitest'
import { render, screen, cleanup, act } from '@testing-library/react'
import { useState, useEffect, type ReactNode } from 'react'
import { WebSocketProvider } from '@app/providers/WebSocketProvider'
import { useWebSocket } from '@app/shared/model/websocket.context'
import { AuthContext } from '@app/shared/model/auth.context'
import type { AuthManager } from '@app/shared/auth/AuthManager'

afterEach(() => {
  cleanup()
  localStorage.clear()
  vi.unstubAllGlobals()
})

class MockWebSocket {
  static instances: MockWebSocket[] = []
  static OPEN = 1
  url: string
  readyState = 0
  onopen: (() => void) | null = null
  onclose: (() => void) | null = null
  onmessage: ((e: { data: string }) => void) | null = null
  onerror: (() => void) | null = null
  send = vi.fn()
  close = vi.fn(() => {
    this.readyState = 3
  })

  constructor(url: string) {
    this.url = url
    MockWebSocket.instances.push(this)
  }

  simulateOpen() {
    this.readyState = 1
    this.onopen?.()
  }

  simulateMessage(data: unknown) {
    this.onmessage?.({ data: JSON.stringify(data) })
  }

  simulateClose() {
    this.readyState = 3
    this.onclose?.()
  }
}

// Minimal AuthManager stub: the provider only uses `onAccessTokenChange`. The
// returned `fireTokenChange` drives login/refresh/logout from the test.
function createMockAuthManager() {
  let tokenCb: ((token: string | null) => void) | null = null
  const am = {
    onAccessTokenChange: (cb: (token: string | null) => void) => {
      tokenCb = cb
      return () => {
        tokenCb = null
      }
    },
  }
  return {
    authManager: am as unknown as AuthManager,
    fireTokenChange: (token: string | null) => act(() => tokenCb?.(token)),
  }
}

function renderWithAuth(authManager: AuthManager, ui: ReactNode) {
  return render(
    <AuthContext.Provider
      value={{ authManager, user: null, isAuthenticated: true, isLoading: false }}
    >
      {ui}
    </AuthContext.Provider>
  )
}

beforeEach(() => {
  MockWebSocket.instances = []
  localStorage.setItem('af:auth:access', 'test-token')
  vi.stubGlobal('WebSocket', MockWebSocket)
})

function StatusDisplay() {
  const { status, subscribe } = useWebSocket()
  const [lastMsg, setLastMsg] = useState<string>('none')

  useEffect(() => {
    return subscribe((data) => {
      setLastMsg(JSON.stringify(data))
    })
  }, [subscribe])

  return (
    <div>
      <span data-testid="status">{status}</span>
      <span data-testid="message">{lastMsg}</span>
    </div>
  )
}

describe('WebSocketProvider', () => {
  test('shows connecting status initially', () => {
    const { authManager } = createMockAuthManager()
    renderWithAuth(
      authManager,
      <WebSocketProvider url="ws://localhost:4003/ws">
        <StatusDisplay />
      </WebSocketProvider>
    )
    expect(screen.getByTestId('status').textContent).toBe('connecting')
    expect(MockWebSocket.instances).toHaveLength(1)
    expect(new URL(MockWebSocket.instances[0].url).searchParams.get('token')).toBe('test-token')
  })

  test('stays disconnected without an auth token', () => {
    localStorage.removeItem('af:auth:access')
    const { authManager } = createMockAuthManager()
    renderWithAuth(
      authManager,
      <WebSocketProvider url="ws://localhost:4003/ws">
        <StatusDisplay />
      </WebSocketProvider>
    )
    expect(screen.getByTestId('status').textContent).toBe('disconnected')
    expect(MockWebSocket.instances).toHaveLength(0)
  })

  test('shows connected after open', () => {
    const { authManager } = createMockAuthManager()
    renderWithAuth(
      authManager,
      <WebSocketProvider url="ws://localhost:4003/ws">
        <StatusDisplay />
      </WebSocketProvider>
    )
    act(() => {
      MockWebSocket.instances[0].simulateOpen()
    })
    expect(screen.getByTestId('status').textContent).toBe('connected')
  })

  test('receives messages', () => {
    const { authManager } = createMockAuthManager()
    renderWithAuth(
      authManager,
      <WebSocketProvider url="ws://localhost:4003/ws">
        <StatusDisplay />
      </WebSocketProvider>
    )
    act(() => {
      MockWebSocket.instances[0].simulateOpen()
      MockWebSocket.instances[0].simulateMessage({ type: 'task.progress', taskId: '1' })
    })
    expect(screen.getByTestId('message').textContent).toContain('task.progress')
  })

  test('shows disconnected after close', () => {
    const { authManager } = createMockAuthManager()
    renderWithAuth(
      authManager,
      <WebSocketProvider url="ws://localhost:4003/ws">
        <StatusDisplay />
      </WebSocketProvider>
    )
    act(() => {
      MockWebSocket.instances[0].simulateOpen()
      MockWebSocket.instances[0].simulateClose()
    })
    expect(screen.getByTestId('status').textContent).toBe('disconnected')
  })

  // F070: auth reactivity on the live connection.
  test('logout closes the socket and does not reconnect', () => {
    const { authManager, fireTokenChange } = createMockAuthManager()
    renderWithAuth(
      authManager,
      <WebSocketProvider url="ws://localhost:4003/ws">
        <StatusDisplay />
      </WebSocketProvider>
    )
    act(() => MockWebSocket.instances[0].simulateOpen())
    const socket = MockWebSocket.instances[0]

    fireTokenChange(null)

    expect(socket.close).toHaveBeenCalled()
    expect(screen.getByTestId('status').textContent).toBe('disconnected')
    // No replacement socket — the prior session's connection is gone for good.
    expect(MockWebSocket.instances).toHaveLength(1)
  })

  test('a refreshed token reconnects with the new token', () => {
    const { authManager, fireTokenChange } = createMockAuthManager()
    renderWithAuth(
      authManager,
      <WebSocketProvider url="ws://localhost:4003/ws">
        <StatusDisplay />
      </WebSocketProvider>
    )
    act(() => MockWebSocket.instances[0].simulateOpen())
    const oldSocket = MockWebSocket.instances[0]

    // AuthManager has persisted the refreshed token before notifying listeners.
    localStorage.setItem('af:auth:access', 'new-token')
    fireTokenChange('new-token')

    expect(oldSocket.close).toHaveBeenCalled()
    expect(MockWebSocket.instances).toHaveLength(2)
    expect(new URL(MockWebSocket.instances[1].url).searchParams.get('token')).toBe('new-token')
  })

  test('an unchanged token does not rebuild the socket', () => {
    const { authManager, fireTokenChange } = createMockAuthManager()
    renderWithAuth(
      authManager,
      <WebSocketProvider url="ws://localhost:4003/ws">
        <StatusDisplay />
      </WebSocketProvider>
    )
    act(() => MockWebSocket.instances[0].simulateOpen())

    // e.g. a user-profile update persists without changing the access token.
    fireTokenChange('test-token')

    expect(MockWebSocket.instances).toHaveLength(1)
  })
})
