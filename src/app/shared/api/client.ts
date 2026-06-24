function getAuthToken(): string | null {
  try {
    return localStorage.getItem('af:auth:access')
  } catch {
    return null
  }
}

export async function apiFetch<T>(url: string, init?: RequestInit): Promise<T> {
  const token = typeof window !== 'undefined' ? getAuthToken() : null
  const res = await fetch(url, {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      ...(init?.headers ?? {}),
    },
  })
  if (!res.ok) {
    const body = await res.text().catch(() => '')
    throw new Error(`API ${res.status}: ${body}`)
  }
  return res.json() as Promise<T>
}
