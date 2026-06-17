import { afterEach, describe, expect, test } from 'vitest'
import { LegalPage } from '@app/shared/ui/legal/LegalPage'

let page: LegalPage | null = null

afterEach(() => {
  page?.destroy()
  page = null
  document.body.innerHTML = ''
  history.replaceState(null, '', '/')
})

describe('LegalPage', () => {
  test('explains why first-time users are reviewing legal pages', () => {
    page = new LegalPage()
    page.show('privacy')

    expect(document.querySelector('.legal-summary')?.textContent).toContain(
      'Review what you agree to and how your workspace data is handled'
    )
    expect(document.querySelector('.legal-tab.active')?.textContent).toContain('Privacy Policy')
  })

  test('describes the current service without old product or deep infrastructure labels', () => {
    page = new LegalPage()
    page.show('terms')

    const text = document.body.textContent ?? ''
    expect(text).toContain('self-hosted governed AI workbench for teams')
    expect(text).toContain('Agent management for creating, starting, stopping, and reviewing')
    expect(text).toContain('Result records, saved notes, and saved instructions')
    expect(text).toContain('Setup tools for troubleshooting and supported automation')
    expect(text).not.toContain('Operator tools')
    expect(text).not.toContain('3D visualization platform')
    expect(text).not.toContain('Claude Code')
    expect(text).not.toContain('LLM gateway')
    expect(text).not.toContain('WebSocket-based')
    expect(text).not.toMatch(/context\s+review\s+tools/i)
  })

  test('keeps privacy and security details readable for non-specialists', () => {
    page = new LegalPage()
    page.show('privacy')

    const text = document.body.textContent ?? ''
    expect(text).toContain('Service request records, such as the action requested, status, and time')
    expect(text).toContain('Login sessions are signed and expire automatically')
    expect(text).toContain('Saved login sessions and access keys are revoked')
    expect(text).toContain('Visual workspace preferences, such as saved view settings')
    expect(text).toContain('Navigation and layout preferences')
    expect(text).toContain('Login session data that keeps you signed in')
    expect(text).not.toContain('endpoint, method')
    expect(text).not.toContain('PostgreSQL')
    expect(text).not.toContain('AES-256')
    expect(text).not.toContain('JWT')
    expect(text).not.toContain('Redis sliding window')
    expect(text).not.toContain('GET /users/me/export')
    expect(text).not.toContain('DELETE /users/me')
    expect(text).not.toContain('login agent')
    expect(text).not.toContain('Draw mode')
    expect(text).not.toContain('Zone elevation')
    expect(text).not.toContain('Refresh tokens')
    expect(text).not.toContain('SSO')
  })
})
