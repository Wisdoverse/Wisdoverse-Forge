import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import '@app/i18n'
import { SidebarNav } from '@app/layouts/sidebar/SidebarNav'
import { useContextFeaturesStore } from '@app/shared/model/context-features.store'
import { useContextStore } from '@app/shared/model/context.store'

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
})

afterEach(() => {
  cleanup()
  useContextFeaturesStore.getState().reset()
  useContextStore.getState().reset()
})

describe('SidebarNav', () => {
  test('labels collapsed icon navigation with the purpose of each surface', () => {
    render(<SidebarNav expanded={false} activePath="/tasks" onNavigate={() => {}} />)

    expect(
      screen.getByRole('button', { name: /start: follow the setup checklist/i })
    ).toHaveAttribute('title', 'Start: follow the setup checklist')
    expect(
      screen.getByRole('button', { name: /tasks: create and review agent work/i })
    ).toHaveAttribute('aria-current', 'page')
    expect(
      screen.getByRole('button', { name: /context: approve reusable knowledge/i })
    ).toBeInTheDocument()
    expect(
      screen.getByRole('button', { name: /agents: create and manage workers/i })
    ).toBeInTheDocument()
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
        name: /settings: configure workspace, runtime, and access/i,
      })
    ).toHaveAttribute('aria-current', 'page')
    expect(
      screen.getByRole('button', {
        name: /admin: manage organizations, users, and system health/i,
      })
    ).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /logout: sign out of this workspace/i }))

    expect(logoutMock).toHaveBeenCalledTimes(1)
    expect(navigateMock).toHaveBeenCalledWith({ to: '/login', search: {} })
  })

  const adminItem = { name: /admin: manage organizations, users, and system health/i }

  test('owners see the admin link (matches the backend require_admin gate)', () => {
    authState.role = 'owner'
    render(<SidebarNav expanded={false} activePath="/settings" onNavigate={() => {}} section="secondary" />)
    expect(screen.getByRole('button', adminItem)).toBeInTheDocument()
  })

  test('non-admin/owner roles do not see the admin link', () => {
    for (const role of ['member', 'viewer', 'user']) {
      authState.role = role
      render(<SidebarNav expanded={false} activePath="/settings" onNavigate={() => {}} section="secondary" />)
      expect(screen.queryByRole('button', adminItem)).not.toBeInTheDocument()
      cleanup()
    }
  })
})
