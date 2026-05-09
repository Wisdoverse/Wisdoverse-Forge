import { useEffect, useRef, useState, type ReactNode } from 'react'
import { AuthManager, type AuthUser } from '@app/shared/auth/AuthManager'
import { initLegacyApis, resetLegacyApis } from '@app/shared/api/legacy'
import { AuthContext } from '@app/shared/model/auth.context'

const apiUrl = `${window.location.protocol}//${window.location.host}/api/v1`

export function AuthProvider({ children }: { children: ReactNode }) {
  const authManagerRef = useRef<AuthManager>(new AuthManager(apiUrl))
  const [user, setUser] = useState<AuthUser | null>(authManagerRef.current.getUser())
  const [isAuthenticated, setIsAuthenticated] = useState(authManagerRef.current.isAuthenticated())
  const [isLoading, setIsLoading] = useState(true)

  useEffect(() => {
    const am = authManagerRef.current

    am.onAuthChange((authenticated) => {
      setIsAuthenticated(authenticated)
      setUser(authenticated ? am.getUser() : null)
      if (authenticated) {
        initLegacyApis(am, () => am.logout())
      } else {
        resetLegacyApis()
      }
    })

    // Check initial auth state - try refresh if we have a refresh token but expired access
    async function checkAuth() {
      if (am.isAuthenticated()) {
        initLegacyApis(am, () => am.logout())
        setIsAuthenticated(true)
        setUser(am.getUser())
        setIsLoading(false)
        return
      }

      // Refresh token lives in an httpOnly cookie — always attempt refresh; server 401s if invalid.
      const success = await am.refreshTokens()
      if (success) {
        initLegacyApis(am, () => am.logout())
        setIsAuthenticated(true)
        setUser(am.getUser())
      } else {
        setIsAuthenticated(false)
        setUser(null)
      }
      setIsLoading(false)
    }

    void checkAuth()

    return () => {
      am.dispose()
    }
  }, [])

  return (
    <AuthContext.Provider
      value={{
        authManager: authManagerRef.current,
        user,
        isAuthenticated,
        isLoading,
      }}
    >
      {children}
    </AuthContext.Provider>
  )
}
