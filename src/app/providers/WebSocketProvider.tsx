import { useEffect, useRef, useState, useCallback, type ReactNode } from 'react'
import {
  WebSocketContext,
  type MessageHandler,
  type WsStatus,
} from '@app/shared/model/websocket.context'
import { useAuth } from '@app/shared/model/auth.context'

interface WebSocketProviderProps {
  url: string
  children: ReactNode
  reconnectInterval?: number
}

export function WebSocketProvider({
  url,
  children,
  reconnectInterval = 3000,
}: WebSocketProviderProps) {
  const { authManager } = useAuth()
  const [status, setStatus] = useState<WsStatus>('connecting')
  const wsRef = useRef<WebSocket | null>(null)
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const backoffRef = useRef(reconnectInterval)
  const closedIntentionallyRef = useRef(false)
  const connectedTokenRef = useRef<string | null>(null)
  const handlersRef = useRef<Set<MessageHandler>>(new Set())
  const MAX_BACKOFF = 30000

  const subscribe = useCallback((handler: MessageHandler) => {
    handlersRef.current.add(handler)
    return () => {
      handlersRef.current.delete(handler)
    }
  }, [])

  // Tear down the live socket and detach its handlers so its async `onclose`
  // cannot schedule a reconnect for a connection we are deliberately replacing.
  const teardownSocket = useCallback(() => {
    if (reconnectTimerRef.current) {
      clearTimeout(reconnectTimerRef.current)
      reconnectTimerRef.current = null
    }
    const ws = wsRef.current
    wsRef.current = null
    if (ws) {
      ws.onopen = null
      ws.onmessage = null
      ws.onerror = null
      ws.onclose = null
      ws.close()
    }
  }, [])

  const urlRef = useRef(url)
  const reconnectIntervalRef = useRef(reconnectInterval)
  useEffect(() => {
    urlRef.current = url
    reconnectIntervalRef.current = reconnectInterval
  }, [url, reconnectInterval])

  const connect = useCallback(() => {
    closedIntentionallyRef.current = false

    const token = localStorage.getItem('af:auth:access')
    if (!token) {
      setStatus('disconnected')
      if (reconnectTimerRef.current) clearTimeout(reconnectTimerRef.current)
      reconnectTimerRef.current = setTimeout(connect, backoffRef.current)
      backoffRef.current = Math.min(backoffRef.current * 2, MAX_BACKOFF)
      return
    }

    const wsUrl = new URL(urlRef.current, window.location.href)
    wsUrl.searchParams.set('token', token)

    const ws = new WebSocket(wsUrl.toString())
    wsRef.current = ws
    connectedTokenRef.current = token
    setStatus('connecting')

    ws.onopen = () => {
      setStatus('connected')
      backoffRef.current = reconnectIntervalRef.current
    }

    ws.onmessage = (event) => {
      let data: unknown
      try {
        data = JSON.parse(event.data)
      } catch {
        data = event.data
      }
      for (const handler of handlersRef.current) {
        handler(data)
      }
    }

    ws.onclose = () => {
      setStatus('disconnected')
      wsRef.current = null
      if (!closedIntentionallyRef.current) {
        if (reconnectTimerRef.current) clearTimeout(reconnectTimerRef.current)
        reconnectTimerRef.current = setTimeout(connect, backoffRef.current)
        backoffRef.current = Math.min(backoffRef.current * 2, MAX_BACKOFF)
      }
    }

    ws.onerror = () => {
      ws.close()
    }
  }, [])

  useEffect(() => {
    connect()
    return () => {
      closedIntentionallyRef.current = true
      if (reconnectTimerRef.current) clearTimeout(reconnectTimerRef.current)
      wsRef.current?.close()
    }
  }, [connect])

  // F070: react to auth changes on the LIVE connection. A logout closes the
  // socket so it stops receiving the prior session's broadcasts and does not
  // reconnect; a new or refreshed token rebuilds the socket so it authenticates
  // with the current token instead of running on a stale one. (`onAccessTokenChange`
  // is the only signal that fires on a silent refresh — `onAuthChange` does not.)
  useEffect(
    () =>
      authManager.onAccessTokenChange((token) => {
        if (token === null) {
          closedIntentionallyRef.current = true
          connectedTokenRef.current = null
          teardownSocket()
          setStatus('disconnected')
          return
        }
        if (token === connectedTokenRef.current) return
        backoffRef.current = reconnectIntervalRef.current
        teardownSocket()
        connect()
      }),
    [authManager, connect, teardownSocket]
  )

  const send = useCallback((data: unknown) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify(data))
    }
  }, [])

  return (
    <WebSocketContext.Provider value={{ status, send, subscribe }}>
      {children}
    </WebSocketContext.Provider>
  )
}
