import { describe, test, expect, afterEach, vi, beforeEach } from 'vitest'
import { render, screen, cleanup, act } from '@testing-library/react'
import { useState, useEffect } from 'react'
import { WebSocketProvider } from '@app/providers/WebSocketProvider'
import { useWebSocket } from '@app/shared/model/websocket.context'

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
  close = vi.fn()

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
    render(
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
    render(
      <WebSocketProvider url="ws://localhost:4003/ws">
        <StatusDisplay />
      </WebSocketProvider>
    )
    expect(screen.getByTestId('status').textContent).toBe('disconnected')
    expect(MockWebSocket.instances).toHaveLength(0)
  })

  test('shows connected after open', () => {
    render(
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
    render(
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
    render(
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
})
