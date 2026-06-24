import { useNavigate, useRouterState } from '@tanstack/react-router'
import { useEffect, useRef } from 'react'
import { AuthPage } from '@app/features/auth'
import { getResetTokenFromLocation } from '@app/shared/lib/publicAuth'
import { useAuth } from '@app/shared/model/auth.context'

function removeAuthPageDom() {
  document.querySelectorAll('.auth-page, #auth-page').forEach((element) => element.remove())
}

export function LoginPage() {
  const containerRef = useRef<HTMLDivElement>(null)
  const authPageRef = useRef<AuthPage | null>(null)
  const navigate = useNavigate()
  const { authManager } = useAuth()
  const resetToken = useRouterState({
    select: (state) => getResetTokenFromLocation(state.location),
  })
  const initialResetTokenRef = useRef<string | null>(resetToken)
  if (!initialResetTokenRef.current && resetToken) {
    initialResetTokenRef.current = resetToken
  }

  useEffect(() => {
    let cancelled = false
    const authPage = new AuthPage(authManager, 'login', initialResetTokenRef.current)
    authPageRef.current = authPage

    const authPromise = authPage.waitForAuth()

    // AuthPage appends to document.body. React StrictMode can mount, clean up,
    // then re-mount while AuthPage.show() is still awaiting provider metadata,
    // so guard stale async completions before they leave duplicate auth DOM.
    removeAuthPageDom()
    void authPage
      .show()
      .then(() => {
        if (cancelled) {
          authPage.hide()
          removeAuthPageDom()
        }
      })
      .catch((error) => {
        console.error('Failed to show auth page:', error)
      })

    void authPromise.then(() => {
      if (cancelled) return
      // Full page reload after login keeps the vanilla AuthPage handoff simple.
      window.location.href = '/'
    })

    return () => {
      cancelled = true
      authPage.hide()
      removeAuthPageDom()
      authPageRef.current = null
    }
  }, [authManager, navigate])

  return <div ref={containerRef} />
}
