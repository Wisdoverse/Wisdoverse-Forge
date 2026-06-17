import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import '@app/i18n'
import { SidebarNav } from '@app/layouts/sidebar/SidebarNav'
import { useContextFeaturesStore } from '@app/shared/model/context-features.store'
import { useContextStore } from '@app/shared/model/context.store'
import { useSettingsStore } from '@app/shared/model/settings.store'

const navigateMock = vi.fn()
const logoutMock = vi.fn()
const authState = vi.hoisted(() => ({ role: 'admin' as string }))

vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => navigateMock,
}))

vi.mock('@app/shared/model/auth.context', () => ({
  useAuth: () => ({
    authManager: { logout: logoutMock },
    user: { role: authState.role },
    isAuthenticated: true,
    isLoading: false,
  }),
}))

beforeEach(() => {
  vi.clearAllMocks()
  authState.role = 'admin'
  useContextFeaturesStore.setState({
    governance: true,
    preview: false,
    injection: false,
    analytics: false,
    loaded: true,
    loading: false,
  })
  useContextStore.getState().reset()
  useSettingsStore.setState({ preferences: null, preferencesLoaded: false })
})

afterEach(() => {
  cleanup()
  useContextFeaturesStore.getState().reset()
  useContextStore.getState().reset()
  useSettingsStore.setState({ preferences: null, preferencesLoaded: false })
})

describe('SidebarNav', () => {
  const previousSavedItemNavCopy = new RegExp(
    ['context:', 'review', 'saved', 'memories', 'and', 'instructions'].join('\\s+'),
    'i'
  )

  test('labels collapsed icon navigation with the purpose of each surface', () => {
    render(<SidebarNav expanded={false} activePath="/tasks" onNavigate={() => {}} />)

    expect(
      screen.getByRole('button', { name: /setup checklist: follow the setup checklist/i })
    ).toHaveAttribute('title', 'Setup checklist: follow the setup checklist')
    expect(
      screen.getByRole('button', { name: /tasks: see tasks and review progress/i })
    ).toHaveAttribute('aria-current', 'page')
    expect(
      screen.getByRole('button', { name: /saved items: review saved notes and instructions/i })
    ).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /context: review saved guidance/i })).toBeNull()
    expect(screen.queryByRole('button', { name: previousSavedItemNavCopy })).toBeNull()
    expect(
      screen.getByRole('button', { name: /agents: create and manage agents/i })
    ).toBeInTheDocument()
    expect(
      screen.getByRole('button', {
        name: /saved instructions: reuse instructions/i,
      })
    ).toBeInTheDocument()
    expect(
      screen.queryByRole('button', { name: /skills: reuse proven work steps/i })
    ).not.toBeInTheDocument()
  })

  test('labels secondary navigation and signs out from the workspace', () => {
    render(
      <SidebarNav
        expanded={false}
        activePath="/settings"
        onNavigate={() => {}}
        section="secondary"
      />
    )

    expect(
      screen.getByRole('button', {
        name: /settings: manage workspace, agents, and access/i,
      })
    ).toHaveAttribute('aria-current', 'page')
    expect(
      screen.getByRole('button', {
        name: /admin: manage team spaces, users, and system health/i,
      })
    ).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /logout: sign out of this workspace/i }))

    expect(logoutMock).toHaveBeenCalledTimes(1)
    expect(navigateMock).toHaveBeenCalledWith({ to: '/login', search: {} })
  })

  const adminItem = { name: /admin: manage team spaces, users, and system health/i }

  test('owners see the admin link (matches the backend require_admin gate)', () => {
    authState.role = 'owner'
    render(
      <SidebarNav
        expanded={false}
        activePath="/settings"
        onNavigate={() => {}}
        section="secondary"
      />
    )
    expect(screen.getByRole('button', adminItem)).toBeInTheDocument()
  })

  test('non-admin/owner roles do not see the admin link', () => {
    for (const role of ['member', 'viewer', 'user']) {
      authState.role = role
      render(
        <SidebarNav
          expanded={false}
          activePath="/settings"
          onNavigate={() => {}}
          section="secondary"
        />
      )
      expect(screen.queryByRole('button', adminItem)).not.toBeInTheDocument()
      cleanup()
    }
  })

  const startItem = { name: /setup checklist: follow the setup checklist/i }

  test('hides the Getting Started entry once the guide is dismissed', () => {
    useSettingsStore.setState({
      preferences: { gettingStartedDismissed: true },
      preferencesLoaded: true,
    })

    render(<SidebarNav expanded={true} activePath="/tasks" onNavigate={() => {}} />)

    expect(screen.queryByRole('button', startItem)).not.toBeInTheDocument()
    // The rest of the primary navigation is unaffected.
    expect(
      screen.getByRole('button', { name: /tasks: see tasks and review progress/i })
    ).toBeInTheDocument()
  })

  test('hides the Getting Started entry immediately after a skip preference update', () => {
    useSettingsStore.setState({
      preferences: { gettingStartedDismissed: false },
      preferencesLoaded: true,
    })

    render(<SidebarNav expanded={true} activePath="/tasks" onNavigate={() => {}} />)

    expect(screen.getByRole('button', startItem)).toBeInTheDocument()

    act(() => {
      useSettingsStore.setState({
        preferences: { gettingStartedDismissed: true },
        preferencesLoaded: true,
      })
    })

    expect(screen.queryByRole('button', startItem)).not.toBeInTheDocument()
  })

  test('keeps showing the Getting Started entry while preferences are unknown', () => {
    // preferences: null (request not finished) — do not blank the nav slot.
    render(<SidebarNav expanded={true} activePath="/tasks" onNavigate={() => {}} />)

    expect(screen.getByRole('button', startItem)).toBeInTheDocument()
  })

  test('keeps showing the Getting Started entry when the stored preference is false', () => {
    useSettingsStore.setState({
      preferences: { gettingStartedDismissed: false },
      preferencesLoaded: true,
    })

    render(<SidebarNav expanded={true} activePath="/tasks" onNavigate={() => {}} />)

    expect(screen.getByRole('button', startItem)).toBeInTheDocument()
  })
})
