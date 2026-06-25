import { authFetch } from './authFetch'

export async function apiFetch<T>(url: string, init?: RequestInit): Promise<T> {
  // F068/F075: route through the shared auth-aware fetch (token injection +
  // 401 refresh/retry) instead of stamping the token by hand, so a mid-session
  // token expiry recovers transparently instead of surfacing a raw `API 401`.
  const res = await authFetch(url, {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      ...(init?.headers ?? {}),
    },
  })
  if (!res.ok) {
    const body = await res.text().catch(() => '')
    throw new Error(`API ${res.status}: ${body}`)
  }
  return res.json() as Promise<T>
}
