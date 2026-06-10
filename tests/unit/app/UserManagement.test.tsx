import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { UserManagement } from '@app/features/admin/UserManagement'
import { useAdminStore, type AdminUser } from '@app/shared/model/admin.store'

const originalAdminState = useAdminStore.getState()

const mockUser: AdminUser = {
  id: 'user-1',
  email: 'alex@example.com',
  displayName: 'Alex Operator',
  role: 'admin',
  status: 'active',
  createdAt: '2026-05-01T12:00:00.000Z',
  lastLoginAt: null,
}

afterEach(() => {
  cleanup()
  useAdminStore.setState(originalAdminState, true)
  vi.restoreAllMocks()
})

describe('UserManagement', () => {
  test('shows user access in plain language', async () => {
    const loadUsers = vi.fn()
    useAdminStore.setState({
      ...originalAdminState,
      users: [mockUser, { ...mockUser, id: 'user-2', role: 'member', status: 'inactive' }],
      usersTotal: 2,
      usersPage: 1,
      usersLoading: false,
      usersError: null,
      userSearch: '',
      loadUsers,
    })

    render(<UserManagement />)

    await waitFor(() => expect(loadUsers).toHaveBeenCalledWith(1))
    expect(screen.getByText('User access')).toBeDefined()
    expect(screen.getByText(/Access levels are read-only in this view/i)).toBeDefined()
    expect(screen.getByText('Person')).toBeDefined()
    expect(screen.getByText('Access level')).toBeDefined()
    expect(screen.getByText('Sign-in status')).toBeDefined()
    expect(screen.getByText('Added')).toBeDefined()
    expect(screen.getByText('Last sign-in')).toBeDefined()
    expect(screen.getByText('Admin')).toBeDefined()
    expect(screen.getByText('Member')).toBeDefined()
    expect(screen.getByText('Can sign in')).toBeDefined()
    expect(screen.getByText('Access paused')).toBeDefined()
    // The fabricated sessions column is gone — the backend never reported it.
    expect(screen.queryByText('Active sessions')).toBeNull()
  })

  test('renders access levels as read-only chips without an editor', () => {
    useAdminStore.setState({
      ...originalAdminState,
      users: [mockUser],
      usersTotal: 1,
      usersPage: 1,
      usersLoading: false,
      usersError: null,
      userSearch: '',
      loadUsers: vi.fn(),
    })

    render(<UserManagement />)

    expect(screen.getByText('Admin')).toBeDefined()
    expect(screen.getByText('Can manage users, settings, and system configuration.')).toBeDefined()
    // No role editor remains: no select, no save button, no change-access affordance.
    expect(screen.queryByRole('combobox')).toBeNull()
    expect(screen.queryByRole('button', { name: /save role/i })).toBeNull()
    expect(screen.queryByTitle('Change what this user can do')).toBeNull()
  })

  test('explains an empty user search result without crashing on zero users', () => {
    useAdminStore.setState({
      ...originalAdminState,
      users: [],
      usersTotal: 0,
      usersPage: 1,
      usersLoading: false,
      usersError: null,
      userSearch: 'missing@example.com',
      loadUsers: vi.fn(),
    })

    render(<UserManagement />)

    expect(screen.getByText('No users match this view')).toBeDefined()
    expect(screen.getByText(/New teammates appear here after they are invited/i)).toBeDefined()
    // Zero users → no pagination controls and no crash.
    expect(screen.queryByRole('button', { name: 'Previous' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Next' })).toBeNull()
  })

  test('search submits a fresh first-page lookup', async () => {
    const loadUsers = vi.fn()
    useAdminStore.setState({
      ...originalAdminState,
      users: [mockUser],
      usersTotal: 1,
      usersPage: 1,
      usersLoading: false,
      usersError: null,
      userSearch: 'alex',
      loadUsers,
    })

    render(<UserManagement />)
    await waitFor(() => expect(loadUsers).toHaveBeenCalledWith(1))
    loadUsers.mockClear()

    fireEvent.click(screen.getByRole('button', { name: 'Find users' }))

    await waitFor(() => expect(loadUsers).toHaveBeenCalledWith(1))
  })

  test('pagination derives total pages from the user total', async () => {
    const loadUsers = vi.fn()
    useAdminStore.setState({
      ...originalAdminState,
      users: [mockUser],
      usersTotal: 51, // 3 pages at the fixed 25-per-page limit
      usersPage: 2,
      usersLoading: false,
      usersError: null,
      userSearch: '',
      loadUsers,
    })

    render(<UserManagement />)
    await waitFor(() => expect(loadUsers).toHaveBeenCalledWith(1))
    loadUsers.mockClear()

    expect(screen.getByText('Showing page 2 of 3')).toBeDefined()
    const previous = screen.getByRole('button', { name: 'Previous' })
    const next = screen.getByRole('button', { name: 'Next' })
    expect(previous).not.toBeDisabled()
    expect(next).not.toBeDisabled()

    fireEvent.click(next)
    await waitFor(() => expect(loadUsers).toHaveBeenCalledWith(3))
  })
})
