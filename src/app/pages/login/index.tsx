import { useNavigate, useRouterState } from '@tanstack/react-router'
import { useEffect, useRef, useState } from 'react'
import { AuthPage } from '@app/features/auth'
import { getInviteTokenFromLocation, getResetTokenFromLocation } from '@app/shared/lib/publicAuth'
import { teamApi } from '@app/entities/navigation/team/api/teamApi'
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
  const inviteToken = useRouterState({
    select: (state) => getInviteTokenFromLocation(state.location),
  })
  const [inviteNotice, setInviteNotice] = useState<string | null>(null)

  useEffect(() => {
    if (!inviteToken) return
    let cancelled = false
    let attempted = false
    authManager.onAuthChange((authenticated) => {
      if (!authenticated || attempted || inviteNotice) return
      attempted = true
      void teamApi
        .redeemInvite(inviteToken)
        .then(() => {
          if (!cancelled) setInviteNotice('You joined the team. Welcome!')
        })
        .catch(() => {
          if (!cancelled) {
            setInviteNotice('The invite could not be redeemed — ask the team lead for a new one.')
          }
        })
    })
    return () => {
      cancelled = true
    }
  }, [inviteToken, inviteNotice, authManager])
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

  return (
    <div ref={containerRef}>
      {inviteNotice && (
        <div
          data-testid="invite-notice"
          role="status"
          className="fixed inset-x-0 top-0 z-[70] flex justify-center px-4 pt-4"
        >
          <p className="rounded-card border border-apple-blue/25 bg-white px-4 py-2 text-ui-caption font-medium text-foreground-light shadow-lg dark:bg-surface-dark dark:text-foreground-dark">
            {inviteNotice}
          </p>
        </div>
      )}
    </div>
  )
}
