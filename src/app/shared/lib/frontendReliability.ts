type Identity = { id: string; orgId?: string }
type BrowserSession = {
  id: string
  userId: string
  orgId: string
  token: string
  endpoint: string
  crashed: boolean
  ended: boolean
}

// Document state survives React StrictMode remounts, but never an identity change.
let session: BrowserSession | null = null
let observing = false

function matchesTokenScope(identity: Identity, token: string): boolean {
  try {
    const payload = token.split('.')[1].replace(/-/g, '+').replace(/_/g, '/')
    const claims = JSON.parse(atob(payload))
    // The API verifies the signature; this comparison prevents cached identity drift.
    return claims.sub === identity.id && claims.org === identity.orgId
  } catch {
    return false
  }
}

function send(observation: BrowserSession, kind: 'started' | 'ended' | 'crashed'): void {
  try {
    // Capture credentials once; auth refresh/offline queues must not change the event owner.
    void fetch(observation.endpoint, {
      method: 'POST',
      credentials: 'same-origin',
      keepalive: true,
      headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${observation.token}` },
      body: JSON.stringify({
        event_name: `frontend_session_${kind}`,
        properties: { browserSessionId: observation.id },
      }),
    }).catch(() => undefined)
  } catch {
    // Lost observations remain gaps; telemetry must not interrupt the workbench.
  }
}

function endSession(): void {
  if (!session || session.ended) return
  session.ended = true
  // A lost crash request must never be followed by a healthy end.
  if (!session.crashed) send(session, 'ended')
}

export function recordFrontendCrash(): void {
  if (!session || session.crashed) return
  session.crashed = true
  send(session, 'crashed')
}

export function observeFrontendSession(
  identity: Identity | null,
  token: string | null,
  apiUrl: string
): void {
  try {
    if (!observing) {
      window.addEventListener('error', recordFrontendCrash)
      window.addEventListener('unhandledrejection', recordFrontendCrash)
      window.addEventListener('pagehide', (event) => {
        if (!event.persisted) endSession()
      })
      observing = true
    }
    if (!identity?.id || !identity.orgId || !token || !matchesTokenScope(identity, token)) {
      endSession()
      session = null
      return
    }
    if (session?.userId === identity.id && session.orgId === identity.orgId) {
      session.token = token
      return
    }
    endSession()
    session = {
      id: crypto.randomUUID(),
      userId: identity.id,
      orgId: identity.orgId,
      token,
      endpoint: `${apiUrl.replace(/\/$/, '')}/analytics/events`,
      crashed: false,
      ended: false,
    }
    send(session, 'started')
  } catch {
    // Unsupported observation is a coverage gap, not a reason to fail login.
  }
}
