import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import '@app/i18n'
import { SidebarNav } from '@app/layouts/sidebar/SidebarNav'
import { useContextFeaturesStore } from '@app/entities/context/model/context-features.store'
import { useContextStore } from '@app/features/context/model/context.store'
import { useAgentsStore } from '@app/entities/agent'
import { useNavigationStore } from '@app/entities/navigation'
import { useBoardStore } from '@app/entities/navigation/model/board.store'
import { useSettingsStore } from '@app/entities/settings'
import { useSkillsStore } from '@app/entities/skill'

const navigateMock = vi.fn()
const logoutMock = vi.fn()
const authState = vi.hoisted(() => ({ role: 'admin' as string, isAdmin: true }))
const themeState = vi.hoisted(() => ({ theme: 'light' as 'light' | 'dark', toggleTheme: vi.fn() }))

vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => navigateMock,
}))

vi.mock('@app/shared/model/auth.context', () => ({
  useAuth: () => ({
    authManager: { logout: logoutMock },
    // The Admin link is gated on the server-side platform-admin flag
    // (`users.is_admin`), NOT the per-org role — see #881.
    user: { role: authState.role, isAdmin: authState.isAdmin },
    isAuthenticated: true,
    isLoading: false,
  }),
}))

vi.mock('@app/shared/model/theme.context', () => ({
  useTheme: () => themeState,
}))

beforeEach(() => {
  vi.clearAllMocks()
  authState.role = 'admin'
  authState.isAdmin = true
  themeState.theme = 'light'
  useContextFeaturesStore.setState({
    governance: true,
    preview: false,
    injection: false,
    analytics: false,
    loaded: true,
    loading: false,
  })
  useContextStore.getState().reset()
  useAgentsStore.getState().reset()
  useNavigationStore.getState().reset()
  useBoardStore.getState().reset()
  useSkillsStore.getState().reset()
  useSettingsStore.setState({
    preferences: null,
    preferencesLoaded: false,
    providers: [],
    runtimeSettings: null,
  })
})

afterEach(() => {
  cleanup()
  useContextFeaturesStore.getState().reset()
  useContextStore.getState().reset()
  useAgentsStore.getState().reset()
  useNavigationStore.getState().reset()
  useBoardStore.getState().reset()
  useSkillsStore.getState().reset()
  useSettingsStore.setState({
    preferences: null,
    preferencesLoaded: false,
    providers: [],
    runtimeSettings: null,
  })
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
    ).toBeInTheDocument()
    expect(screen.getByTestId('setup-checklist-nav-badge')).toHaveTextContent('0/8')
    expect(
      screen.getByRole('button', { name: /tasks: see tasks and check progress/i })
    ).toHaveAttribute('aria-current', 'page')
    expect(
      screen.getByRole('button', { name: /context: check saved notes and guidance/i })
    ).toBeInTheDocument()
    expect(
      screen.queryByRole('button', { name: /saved items: check saved notes and instructions/i })
    ).toBeNull()
    expect(screen.queryByRole('button', { name: /context: review saved guidance/i })).toBeNull()
    expect(screen.queryByRole('button', { name: previousSavedItemNavCopy })).toBeNull()
    expect(
      screen.getByRole('button', { name: /agents: create and manage agents/i })
    ).toBeInTheDocument()
    expect(
      screen.getByRole('button', {
        name: /skills: reuse guidance/i,
      })
    ).toBeInTheDocument()
    expect(
      screen.queryByRole('button', {
        name: /saved instructions: reuse instructions/i,
      })
    ).toBeNull()
    expect(
      screen.queryByRole('button', { name: /skills: reuse proven work steps/i })
    ).not.toBeInTheDocument()
  })

  test('does not count a chat-only agent in setup progress', () => {
    useAgentsStore.setState({
      agents: [
        {
          id: 'chat-agent',
          name: 'Chat Agent',
          provider: 'model-service',
          model: 'general-model',
          runtimeKind: 'api',
          status: 'idle',
          tasksCompleted: 0,
          tasksInProgress: 0,
          successRate: 0,
        },
      ],
    })

    render(<SidebarNav expanded={false} activePath="/tasks" onNavigate={() => {}} />)

    expect(screen.getByTestId('setup-checklist-nav-badge')).toHaveTextContent('0/8')
  })

  test('labels secondary navigation and signs out of Forge', () => {
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
        name: /settings: manage teams, agents, and access/i,
      })
    ).toHaveAttribute('aria-current', 'page')
    expect(
      screen.getByRole('button', {
        name: /admin: manage team spaces, people, and app health/i,
      })
    ).toBeInTheDocument()
    expect(
      screen.queryByRole('button', {
        name: /admin: manage team spaces, users, and system health/i,
      })
    ).toBeNull()

    expect(screen.queryByRole('button', { name: /logout: sign out of this workspace/i })).toBeNull()

    const themeButton = screen.getByRole('button', { name: 'Switch to dark mode' })
    const logoutButton = screen.getByRole('button', { name: /logout: sign out of Forge/i })
    expect(
      themeButton.compareDocumentPosition(logoutButton) & Node.DOCUMENT_POSITION_FOLLOWING
    ).toBe(4)
    fireEvent.click(themeButton)
    expect(themeState.toggleTheme).toHaveBeenCalledOnce()

    fireEvent.click(logoutButton)

    expect(logoutMock).toHaveBeenCalledTimes(1)
    expect(navigateMock).toHaveBeenCalledWith({ to: '/login', search: {} })
  })

  const adminItem = { name: /admin: manage team spaces, people, and app health/i }

  test('platform admins see the admin link (matches the backend require_platform_admin gate)', () => {
    // is_admin is the gate now, not the per-org role: an org "member" who is a
    // platform admin still sees it.
    authState.role = 'member'
    authState.isAdmin = true
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

  test('non-platform-admins do not see the admin link, even org owners', () => {
    // The previously-vulnerable case: an org "owner" (self-assignable role) that
    // is NOT a platform admin must NOT see the admin link (#881).
    for (const role of ['owner', 'admin', 'member', 'viewer', 'user']) {
      authState.role = role
      authState.isAdmin = false
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
      screen.getByRole('button', { name: /tasks: see tasks and check progress/i })
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

  test('shows the Getting Started entry while preferences are unknown', () => {
    // preferences: null (request not finished) keeps beginner guidance visible.
    render(<SidebarNav expanded={true} activePath="/tasks" onNavigate={() => {}} />)

    expect(screen.getByRole('button', startItem)).toBeInTheDocument()
    expect(
      screen.getByRole('button', { name: /tasks: see tasks and check progress/i })
    ).toBeInTheDocument()
  })

  test('shows the Getting Started entry only when the stored preference is false', () => {
    useSettingsStore.setState({
      preferences: { gettingStartedDismissed: false },
      preferencesLoaded: true,
    })

    render(<SidebarNav expanded={true} activePath="/tasks" onNavigate={() => {}} />)

    expect(screen.getByRole('button', startItem)).toBeInTheDocument()
  })
})
