import type { AgentStatus } from './types'

const AGENT_STATUS_LABELS: Record<string, string> = {
  working: 'Working now',
  idle: 'Ready',
  offline: 'Not connected',
}

export function agentStatusLabel(status: AgentStatus | string | null | undefined): string {
  const normalized = agentStatusKey(status)
  if (!normalized) return 'Status not reported'
  return AGENT_STATUS_LABELS[normalized] ?? 'Status needs review'
}

export function agentStatusKey(status: AgentStatus | string | null | undefined): string {
  return typeof status === 'string' ? status.trim().toLowerCase() : ''
}
