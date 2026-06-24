import { useEffect, useRef, useState, useCallback, type ReactNode } from 'react'
import {
  WebSocketContext,
  type MessageHandler,
  type WsStatus,
} from '@app/shared/model/websocket.context'

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
  const [status, setStatus] = useState<WsStatus>('connecting')
  const wsRef = useRef<WebSocket | null>(null)
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const backoffRef = useRef(reconnectInterval)
  const closedIntentionallyRef = useRef(false)
  const handlersRef = useRef<Set<MessageHandler>>(new Set())
  const MAX_BACKOFF = 30000

  const subscribe = useCallback((handler: MessageHandler) => {
    handlersRef.current.add(handler)
    return () => {
      handlersRef.current.delete(handler)
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
