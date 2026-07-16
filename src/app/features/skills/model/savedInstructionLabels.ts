const RAW_INTERNAL_LABEL_PATTERN = /[/\\@]|(?:^|\s)[a-z0-9]+[._-][a-z0-9._-]*(?:\s|$)/i

export function savedInstructionSourceLabel(source: string, fallback: string): string {
  const label = source
    .replace(/\bworkspace\b/gi, (match) => (match[0] === 'W' ? 'Team space' : 'team space'))

    .replace(/\s+/g, ' ')
    .trim()

  if (!label || RAW_INTERNAL_LABEL_PATTERN.test(label)) return fallback
  return label
}

export function savedInstructionAudienceLabel(source: string, fallback: string): string {
  const label = savedInstructionSourceLabel(source, fallback)
  const normalized = label.toLowerCase()

  if (normalized === 'team space saved instructions' || normalized === 'team space skills')
    return 'This team space'
  if (normalized === 'project saved instructions' || normalized === 'project skills')
    return 'This project'
  if (normalized === 'global saved instructions' || normalized === 'global skills')
    return 'Everyone'
  if (normalized === 'saved instructions' || normalized === 'skills') return 'Skills'

  return label
}

export function knownWorkToolLabel(tool: string): string | null {
  switch (tool.trim().toLowerCase()) {
    case 'claude':
      return 'Claude Code'
    case 'codex':
      return 'Codex'
    case 'gemini':
      return 'Gemini'
    case 'opencode':
      return 'OpenCode'
    default:
      return null
  }
}
