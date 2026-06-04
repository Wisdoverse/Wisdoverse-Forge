export type WorkspaceSettingsAction =
  | 'load-teams'
  | 'create-team'
  | 'load-projects'
  | 'create-project'

const ACTION_LABELS: Record<WorkspaceSettingsAction, string> = {
  'load-teams': 'Teams',
  'create-team': 'The team',
  'load-projects': 'Projects',
  'create-project': 'The project',
}

function parseApiError(error: unknown): { status: number | null; detail: string | null } {
  if (!(error instanceof Error)) return { status: null, detail: null }
  const message = error.message.trim()
  const match = /^API\s+(\d{3}):\s*(.*)$/s.exec(message)
  if (!match) return { status: null, detail: message || null }

  const status = Number(match[1])
  const body = match[2]?.trim()
  if (!body) return { status, detail: null }

  try {
    const parsed = JSON.parse(body) as unknown
    if (parsed && typeof parsed === 'object') {
      const data = parsed as Record<string, unknown>
      if (typeof data.error === 'string' && data.error.trim()) {
        return { status, detail: data.error.trim() }
      }
      if (typeof data.message === 'string' && data.message.trim()) {
        return { status, detail: data.message.trim() }
      }
    }
  } catch {
    // Keep the response text below when the server did not send JSON.
  }

  return { status, detail: body }
}

function isNetworkError(error: unknown): boolean {
  return (
    error instanceof TypeError ||
    (error instanceof Error && /^Failed to fetch$/i.test(error.message.trim()))
  )
}

function actionVerb(action: WorkspaceSettingsAction): string {
  switch (action) {
    case 'load-teams':
      return 'load teams'
    case 'create-team':
      return 'create this team'
    case 'load-projects':
      return 'load projects'
    case 'create-project':
      return 'create this project'
  }
}

export function workspaceSettingsErrorMessage(
  action: WorkspaceSettingsAction,
  error: unknown
): string {
  if (isNetworkError(error)) {
    return `${ACTION_LABELS[action]} could not ${
      action.startsWith('load') ? 'load' : 'be created'
    } because the browser could not reach the server. Check your connection, then try again.`
  }

  const { status, detail } = parseApiError(error)
  const suffix = detail ? ` Details: ${detail}` : ''

  if (!status) {
    return `${ACTION_LABELS[action]} could not ${
      action.startsWith('load') ? 'load' : 'be created'
    }. Refresh Settings and try again.${suffix}`
  }

  const statusText = `Code: ${status}.`

  if (status === 401) {
    return `Sign in again, then ${actionVerb(action)}. ${statusText}${suffix}`
  }

  if (status === 403) {
    if (action === 'create-project') {
      return `You do not have permission to create projects in this team. Ask a team admin to update your project access. ${statusText}${suffix}`
    }
    if (action === 'create-team') {
      return `You do not have permission to create teams. Ask an owner or admin to update your role. ${statusText}${suffix}`
    }
    return `You do not have permission to ${actionVerb(action)}. Ask an owner or admin to update your access. ${statusText}${suffix}`
  }

  if (status === 404) {
    return `The workspace settings service is not available from this page. Refresh after the backend is deployed. ${statusText}${suffix}`
  }

  if (status === 409) {
    return `${ACTION_LABELS[action]} changed or already exists. Refresh Settings, review the current list, then try again. ${statusText}${suffix}`
  }

  if (status === 422) {
    switch (action) {
      case 'create-team':
        return `Check the team name, then try again. ${statusText}${suffix}`
      case 'create-project':
        return `Check the project name and selected team, then try again. ${statusText}${suffix}`
      case 'load-teams':
        return `Check the selected organization, then refresh teams. ${statusText}${suffix}`
      case 'load-projects':
        return `Check the selected organization and teams, then refresh projects. ${statusText}${suffix}`
    }
  }

  if (status === 429) {
    return `The workspace settings service is busy. Wait a moment, then ${actionVerb(action)} again. ${statusText}${suffix}`
  }

  if (status >= 500) {
    return `The workspace settings service had a server problem. Try again after the backend is healthy. ${statusText}${suffix}`
  }

  return `${ACTION_LABELS[action]} could not ${
    action.startsWith('load') ? 'load' : 'be created'
  }. Refresh Settings and try again. ${statusText}${suffix}`
}
