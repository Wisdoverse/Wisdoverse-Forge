const CAPABILITY_COPY: Record<string, string> = {
  analysis: 'analyze the task',
  claude: 'use Claude Code for this work',
  codex: 'use Codex for this work',
  coding: 'change code',
  debugging: 'find and fix problems',
  documentation: 'write documentation',
  gemini: 'use Gemini for this work',
  implementation: 'build the task',
  opencode: 'use OpenCode for this work',
  planning: 'plan the work',
  research: 'research the request',
  review: 'review the result',
  testing: 'check the result',
  writing: 'write or edit text',
}

export function agentCapabilitySummary(capabilities: string[]): string {
  const tasks = Array.from(
    new Set(capabilities.map((capability) => agentCapabilityCopy(capability)))
  ).filter(Boolean)

  if (tasks.length === 0) return 'Ready to take this task'
  return `Can ${joinCapabilityTasks(tasks)}`
}

function agentCapabilityCopy(capability: string): string {
  const normalized = capability.trim().toLowerCase().replace(/[_-]+/g, ' ')
  if (!normalized) return ''
  return CAPABILITY_COPY[normalized] ?? `help with ${normalized}`
}

function joinCapabilityTasks(tasks: string[]): string {
  if (tasks.length === 1) return tasks[0]
  if (tasks.length === 2) return `${tasks[0]} and ${tasks[1]}`
  return `${tasks.slice(0, -1).join(', ')}, and ${tasks[tasks.length - 1]}`
}
