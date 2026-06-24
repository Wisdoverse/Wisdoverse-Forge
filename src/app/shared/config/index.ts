import { DEFAULTS } from '@shared/defaults'

function getAgentPort(): number {
  if (typeof window === 'undefined') return DEFAULTS.SERVER_PORT

  const params = new URLSearchParams(window.location.search)
  const portParam = params.get('port')
  if (portParam) {
    const port = Number.parseInt(portParam, 10)
    if (!Number.isNaN(port)) {
      localStorage.setItem('agentforge-port', portParam)
      return port
    }
  }

  const stored = localStorage.getItem('agentforge-port')
  if (stored) {
    const port = Number.parseInt(stored, 10)
    if (!Number.isNaN(port)) return port
  }

  return DEFAULTS.SERVER_PORT
}

const agentPort = getAgentPort()

export const config = {
  agentPort,
  wsUrl: `${window.location.protocol === 'https:' ? 'wss:' : 'ws:'}//${window.location.hostname}:${agentPort}/ws`,
  apiUrl: `${window.location.protocol}//${window.location.hostname}:${agentPort}/api/v1`,
}
