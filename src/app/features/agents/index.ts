import { lazy } from 'react'

export { AgentConfigTab } from './AgentConfigTab'
export { AgentControlPanel } from './AgentControlPanel'
export { AgentKindBadge } from './AgentKindBadge'
export { AgentListView } from './AgentListView'
export { AgentPluginsTab } from './AgentPluginsTab'
export { AgentTasksTab } from './AgentTasksTab'

// Lazy at the barrel: AgentTerminalTab must stay a dynamic import target so
// xterm stays out of the agents route's initial chunk. A static re-export
// here would fold it into every chunk that imports this barrel.
export const AgentTerminalTab = lazy(() =>
  import('./AgentTerminalTab').then((m) => ({ default: m.AgentTerminalTab }))
)
