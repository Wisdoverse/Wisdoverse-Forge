import type { AgentInfo } from './types'

export function agentToolLabel(tool?: AgentInfo['cliTool']): string {
  switch (tool) {
    case 'claude':
      return 'Claude'
    case 'codex':
      return 'Codex'
    case 'gemini':
      return 'Gemini'
    case 'opencode':
      return 'OpenCode'
    default:
      return 'Work tool'
  }
}

export function agentAiServiceLabel(provider?: string | null): string {
  const label = provider?.trim()
  if (!label) return 'Refresh AI service'
  switch (label.toLowerCase()) {
    case 'anthropic':
      return 'Anthropic AI service'
    case 'openai':
      return 'OpenAI AI service'
    case 'google':
    case 'gemini':
      return 'Google AI service'
    case 'ollama':
      return 'Ollama AI service'
    case 'openrouter':
      return 'OpenRouter AI service'
    case 'together':
      return 'Together AI service'
    case 'openai_compatible':
    case 'openai-compatible':
      return 'OpenAI-compatible service'
  }
  if (label.toLowerCase().includes('service')) return label
  if (/^[a-z0-9]+(?:[_-][a-z0-9]+)+$/i.test(label)) return 'Check AI service'
  return `${label} AI service`
}

export function agentRuntimeLabel(agent: AgentInfo): string {
  if (agent.runtimeKind === 'cli') return `${agentToolLabel(agent.cliTool)} on this computer`
  if (agent.cliTool) return `${agentToolLabel(agent.cliTool)} in a managed workspace`
  return agentAiServiceLabel(agent.provider)
}

export function agentServiceLabel(agent: AgentInfo): string {
  if (agent.cliTool) return agentToolLabel(agent.cliTool)
  return agentAiServiceLabel(agent.provider)
}

export function agentAvatarInitial(agent: AgentInfo): string {
  const label = agent.cliTool ? agentToolLabel(agent.cliTool) : agent.provider
  return label.trim().charAt(0).toUpperCase() || 'A'
}
