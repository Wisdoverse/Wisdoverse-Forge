import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'

// The panel reads the signed-in user to hide self-targeted actions; give the
// tests a controllable identity without mounting the full AuthProvider.
const signedInUser = vi.hoisted(() => ({
  current: { id: 'self-1', email: 'operator@example.com', username: 'operator' } as {
    id: string
    email: string
    username: string
  } | null,
}))

vi.mock('@app/shared/model/auth.context', () => ({
  useAuth: () => ({
    authManager: {},
    user: signedInUser.current,
    isAuthenticated: true,
    isLoading: false,
  }),
}))

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
  signedInUser.current = { id: 'self-1', email: 'operator@example.com', username: 'operator' }
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
    expect(screen.getByText(/Change what each person can do/i)).toBeDefined()
    expect(screen.getByText('Person')).toBeDefined()
    expect(screen.getByText('Access level')).toBeDefined()
    expect(screen.getByText('Sign-in status')).toBeDefined()
    expect(screen.getByText('Added')).toBeDefined()
    expect(screen.getByText('Last sign-in')).toBeDefined()
    expect(screen.getByText('Actions')).toBeDefined()
    expect(screen.getByText('Admin')).toBeDefined()
    expect(screen.getByText('Member')).toBeDefined()
    expect(screen.getByText('Can sign in')).toBeDefined()
    expect(screen.getByText('Access paused')).toBeDefined()
    // The fabricated sessions column is gone — the backend never reported it.
    expect(screen.queryByText('Active sessions')).toBeNull()
  })

  test('explains missing and invalid user dates without placeholder symbols', async () => {
    useAdminStore.setState({
      ...originalAdminState,
      users: [
        {
          ...mockUser,
          id: 'user-missing-dates',
          createdAt: null,
          lastLoginAt: null,
        },
        {
          ...mockUser,
          id: 'user-invalid-dates',
          createdAt: 'not-a-date',
          lastLoginAt: 'not-a-date',
        },
      ],
      usersTotal: 2,
      usersPage: 1,
      usersLoading: false,
      usersError: null,
      userSearch: '',
      loadUsers: vi.fn(),
    })

    render(<UserManagement />)

    expect(screen.getByText('Added date not reported')).toBeDefined()
    expect(screen.getByText('Never signed in')).toBeDefined()
    expect(screen.getByText('Added date needs review')).toBeDefined()
    expect(screen.getByText('Sign-in date needs review')).toBeDefined()
    expect(screen.queryByText('—')).toBeNull()
    expect(screen.queryByText('Invalid Date')).toBeNull()
  })

  test('saving a new access level calls the role update and closes the editor', async () => {
    const updateUserRole = vi.fn(async () => {
      // Mirror the real store: the row is swapped for the saved projection.
      useAdminStore.setState((s) => ({
        users: s.users.map((u) => (u.id === 'user-1' ? { ...u, role: 'member' } : u)),
      }))
      return true
    })
    useAdminStore.setState({
      ...originalAdminState,
      users: [mockUser],
      usersTotal: 1,
      usersPage: 1,
      usersLoading: false,
      usersError: null,
      userActionError: null,
      userSearch: '',
      loadUsers: vi.fn(),
      updateUserRole,
    })

    render(<UserManagement />)

    fireEvent.click(screen.getByRole('button', { name: 'Change access' }))
    const select = screen.getByLabelText(/Access level for Alex Operator/i)
    fireEvent.change(select, { target: { value: 'member' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save access' }))

    await waitFor(() => expect(updateUserRole).toHaveBeenCalledWith('user-1', 'member'))
    // Editor closes and the row reflects the saved role.
    await waitFor(() => expect(screen.queryByRole('combobox')).toBeNull())
    expect(screen.getByText('Member')).toBeDefined()
  })

  test('a backend guard rejection keeps the editor open and shows the reason', async () => {
    const guardMessage =
      'This is the only admin account left. Make another person an admin first, then retry this change.'
    const updateUserRole = vi.fn(async () => {
      useAdminStore.setState({ userActionError: guardMessage })
      return false
    })
    useAdminStore.setState({
      ...originalAdminState,
      users: [mockUser],
      usersTotal: 1,
      usersPage: 1,
      usersLoading: false,
      usersError: null,
      userActionError: null,
      userSearch: '',
      loadUsers: vi.fn(),
      updateUserRole,
    })

    render(<UserManagement />)

    fireEvent.click(screen.getByRole('button', { name: 'Change access' }))
    fireEvent.change(screen.getByLabelText(/Access level for Alex Operator/i), {
      target: { value: 'member' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Save access' }))

    await waitFor(() => expect(screen.getByText(guardMessage)).toBeDefined())
    // The editor stays open so the operator can cancel or retry.
    expect(screen.getByRole('combobox')).toBeDefined()
  })

  test('removing a user asks for confirmation before calling delete', async () => {
    const deleteUser = vi.fn(async () => true)
    useAdminStore.setState({
      ...originalAdminState,
      users: [{ ...mockUser, id: 'user-2', displayName: 'Bo Member', role: 'member' }],
      usersTotal: 1,
      usersPage: 1,
      usersLoading: false,
      usersError: null,
      userActionError: null,
      userSearch: '',
      loadUsers: vi.fn(),
      deleteUser,
    })

    render(<UserManagement />)

    fireEvent.click(screen.getByRole('button', { name: 'Remove' }))
    // Nothing is deleted until the confirm step.
    expect(deleteUser).not.toHaveBeenCalled()
    expect(screen.getByText(/Bo Member loses sign-in access right away/i)).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: 'Remove account' }))
    await waitFor(() => expect(deleteUser).toHaveBeenCalledWith('user-2'))
  })

  test('cancelling the remove confirmation keeps the account', () => {
    const deleteUser = vi.fn(async () => true)
    useAdminStore.setState({
      ...originalAdminState,
      users: [{ ...mockUser, id: 'user-2', role: 'member' }],
      usersTotal: 1,
      usersPage: 1,
      usersLoading: false,
      usersError: null,
      userActionError: null,
      userSearch: '',
      loadUsers: vi.fn(),
      deleteUser,
    })

    render(<UserManagement />)

    fireEvent.click(screen.getByRole('button', { name: 'Remove' }))
    fireEvent.click(screen.getByRole('button', { name: 'Keep account' }))

    expect(deleteUser).not.toHaveBeenCalled()
    expect(screen.getByRole('button', { name: 'Remove' })).toBeDefined()
  })

  test('your own row offers no role editor and no remove action', () => {
    signedInUser.current = { id: 'user-1', email: 'alex@example.com', username: 'alex' }
    useAdminStore.setState({
      ...originalAdminState,
      users: [mockUser, { ...mockUser, id: 'user-2', displayName: 'Bo Member', role: 'member' }],
      usersTotal: 2,
      usersPage: 1,
      usersLoading: false,
      usersError: null,
      userActionError: null,
      userSearch: '',
      loadUsers: vi.fn(),
    })

    render(<UserManagement />)

    // Own row: explained, not just missing.
    expect(screen.getByText(/This is you/i)).toBeDefined()
    // The other row still has both actions — exactly one of each.
    expect(screen.getAllByRole('button', { name: 'Change access' })).toHaveLength(1)
    expect(screen.getAllByRole('button', { name: 'Remove' })).toHaveLength(1)
  })

  test('explains an empty user search result and lets operators reset it', async () => {
    const loadUsers = vi.fn()
    useAdminStore.setState({
      ...originalAdminState,
      users: [],
      usersTotal: 0,
      usersPage: 1,
      usersLoading: false,
      usersError: null,
      userSearch: 'missing@example.com',
      loadUsers,
    })

    render(<UserManagement />)

    await waitFor(() => expect(loadUsers).toHaveBeenCalledWith(1))
    loadUsers.mockClear()

    expect(screen.getByText('Search did not find a matching person')).toBeDefined()
    expect(screen.getByText(/clear the search to see everyone who can sign in/i)).toBeDefined()
    expect(screen.queryByText('No users match this view')).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: 'Clear search' }))

    expect(screen.getByLabelText('Search people by name or email')).toHaveValue('')
    await waitFor(() => expect(loadUsers).toHaveBeenCalledWith(1))
    // Zero users → no pagination controls and no crash.
    expect(screen.queryByRole('button', { name: 'Previous' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Next' })).toBeNull()
  })

  test('explains a fully empty user list as an invitation starting point', async () => {
    const loadUsers = vi.fn()
    useAdminStore.setState({
      ...originalAdminState,
      users: [],
      usersTotal: 0,
      usersPage: 1,
      usersLoading: false,
      usersError: null,
      userSearch: '',
      loadUsers,
    })

    render(<UserManagement />)

    await waitFor(() => expect(loadUsers).toHaveBeenCalledWith(1))
    expect(screen.getByText('No one is listed yet')).toBeDefined()
    expect(
      screen.getByText(/people appear here after an owner or admin invites them/i)
    ).toBeDefined()
    expect(screen.queryByRole('button', { name: 'Clear search' })).toBeNull()
    expect(screen.queryByText('No users match this view')).toBeNull()
  })

  test('hides raw load errors behind a recovery step', async () => {
    const loadUsers = vi.fn()
    useAdminStore.setState({
      ...originalAdminState,
      users: [],
      usersTotal: 0,
      usersPage: 1,
      usersLoading: false,
      usersError: 'HTTP 503',
      userSearch: '',
      loadUsers,
    })

    render(<UserManagement />)

    await waitFor(() => expect(loadUsers).toHaveBeenCalledWith(1))
    const alert = screen.getByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent('The admin user list could not load.')
    expect(alert).toHaveTextContent(
      'Refresh Admin, then try again. If it still fails, ask an owner or admin to check Admin setup and your role.'
    )
    expect(alert).not.toHaveTextContent('HTTP 503')
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
