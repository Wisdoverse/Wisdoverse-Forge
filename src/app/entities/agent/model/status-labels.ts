import type { AgentStatus } from './types'

const AGENT_STATUS_LABELS: Record<string, string> = {
  working: 'Working',
  idle: 'Ready',
  offline: 'Offline',
}

export function agentStatusLabel(status: AgentStatus | string | null | undefined): string {
  const normalized = agentStatusKey(status)
  if (!normalized) return 'Status not reported'
  return AGENT_STATUS_LABELS[normalized] ?? 'Status needs review'
}

export function agentStatusKey(status: AgentStatus | string | null | undefined): string {
  return typeof status === 'string' ? status.trim().toLowerCase() : ''
}
