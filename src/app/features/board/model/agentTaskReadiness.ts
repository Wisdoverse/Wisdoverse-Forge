export interface AgentTaskReadinessInput {
  status: string
  capabilities?: readonly string[] | null
}

export function agentHasTaskCapability(agent: AgentTaskReadinessInput): boolean {
  if (!Array.isArray(agent.capabilities)) return true
  return agent.capabilities.some((capability) => capability.trim().length > 0)
}

export function agentCanTakeTask(agent: AgentTaskReadinessInput): boolean {
  if (!agentHasTaskCapability(agent)) return false
  const normalized = normalizeAgentStatus(agent.status)
  return normalized === 'available' || normalized === 'idle'
}

export function agentTaskStatusLabel(agent: AgentTaskReadinessInput): string {
  if (!agentHasTaskCapability(agent)) return 'chat only - cannot take Tasks'

  const normalized = normalizeAgentStatus(agent.status)
  switch (normalized) {
    case 'available':
    case 'idle':
      return 'ready'
    case 'busy':
    case 'working':
      return 'working now'
    case 'offline':
      return 'not connected'
    default:
      return normalized ? 'not ready' : 'check agent status'
  }
}

function normalizeAgentStatus(status: string): string {
  return status.trim().toLowerCase()
}
