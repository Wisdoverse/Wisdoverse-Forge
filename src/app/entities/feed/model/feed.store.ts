import { create } from 'zustand'
import { taskBlockedPreview } from '@app/shared/lib/taskFailureCopy'

export interface FeedItem {
  id: string
  type: string
  agentName: string
  taskTitle: string
  detail: string
  timestamp: number
}

export interface AgentStatus {
  id: string
  name: string
  status: 'working' | 'idle' | 'blocked' | 'offline'
}

export interface AttentionItem {
  id: string
  taskTitle: string
  agentName: string
  reason: string
  timestamp: number
}

export interface Notification {
  id: string
  type:
    | 'blocked'
    | 'completed'
    | 'failed'
    | 'assigned'
    | 'mentioned'
    | 'credential_expired'
    | 'cli_image_updated'
    | 'review_escalated'
  taskId: string
  taskTitle: string
  message: string
  taskHref?: string
  ownerUserId?: string
  read: boolean
  timestamp: number
}

const MAX_FEED_ITEMS = 100
// Cap retained notifications. Distinct-id producers (e.g. the per-version CLI
// image toast) would otherwise grow this list without bound over a long session.
const MAX_NOTIFICATIONS = 100
// F072: cap pending attention items. They are appended (newest last) and only
// removed when explicitly dismissed, so a long session with a steady stream of
// distinct-id items would otherwise grow this list without bound.
const MAX_ATTENTION_ITEMS = 100

interface FeedState {
  feedItems: FeedItem[]
  agents: AgentStatus[]
  attentionItems: AttentionItem[]
  notifications: Notification[]
  addFeedItem: (item: FeedItem) => void
  setAgents: (agents: AgentStatus[]) => void
  addAttentionItem: (item: AttentionItem) => void
  removeAttentionItem: (id: string) => void
  addNotification: (notification: Notification) => void
  markRead: (id: string) => void
  markAllRead: () => void
  reset: () => void
}

const initialState = {
  feedItems: [] as FeedItem[],
  agents: [] as AgentStatus[],
  attentionItems: [] as AttentionItem[],
  notifications: [] as Notification[],
}

export const useFeedStore = create<FeedState>((set) => ({
  ...initialState,
  addFeedItem: (item) =>
    set((s) => ({ feedItems: [item, ...s.feedItems].slice(0, MAX_FEED_ITEMS) })),
  setAgents: (agents) => set({ agents }),
  addAttentionItem: (item) =>
    set((s) => ({
      // Append newest last, then keep only the most recent MAX so the list
      // cannot grow unbounded across a long session (F072).
      attentionItems: [
        ...s.attentionItems,
        { ...item, reason: attentionReasonPreview(item.reason) },
      ].slice(-MAX_ATTENTION_ITEMS),
    })),
  removeAttentionItem: (id) =>
    set((s) => ({ attentionItems: s.attentionItems.filter((a) => a.id !== id) })),
  addNotification: (notification) =>
    set((s) => {
      const existing = s.notifications.find((n) => n.id === notification.id)
      if (existing) {
        return {
          notifications: s.notifications.map((n) =>
            n.id === notification.id ? { ...n, ...notification, read: n.read } : n
          ),
        }
      }
      return { notifications: [notification, ...s.notifications].slice(0, MAX_NOTIFICATIONS) }
    }),
  markRead: (id) =>
    set((s) => ({
      notifications: s.notifications.map((n) => (n.id === id ? { ...n, read: true } : n)),
    })),
  markAllRead: () =>
    set((s) => ({ notifications: s.notifications.map((n) => ({ ...n, read: true })) })),
  reset: () => set(initialState),
}))

export function attentionReasonPreview(rawReason: string): string {
  const reason = rawReason.trim()
  if (!reason || /^blocked$/i.test(reason)) {
    return 'Open the request to see what the agent needs before it can continue.'
  }
  if (/^permission required$/i.test(reason)) {
    return 'Open the details before allowing the agent to continue.'
  }

  return taskBlockedPreview({ blockedHint: reason })
}
