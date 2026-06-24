import { createContext, useContext } from 'react'

export type WsStatus = 'connecting' | 'connected' | 'disconnected'
export type MessageHandler = (data: unknown) => void

export interface WebSocketContextValue {
  status: WsStatus
  send: (data: unknown) => void
  subscribe: (handler: MessageHandler) => () => void
}

export const WebSocketContext = createContext<WebSocketContextValue | null>(null)

export function useWebSocket(): WebSocketContextValue {
  const ctx = useContext(WebSocketContext)
  if (!ctx) throw new Error('useWebSocket must be used within WebSocketProvider')
  return ctx
}
