import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
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
  sessionsCount: 2,
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
      users: [
        mockUser,
        { ...mockUser, id: 'user-2', role: 'viewer', status: 'inactive', sessionsCount: 0 },
      ],
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
    expect(screen.getByText(/Change access only when their job changes/i)).toBeDefined()
    expect(screen.getByText('Person')).toBeDefined()
    expect(screen.getByText('Access level')).toBeDefined()
    expect(screen.getByText('Sign-in status')).toBeDefined()
    expect(screen.getByText('Active sessions')).toBeDefined()
    expect(screen.getByText('Full access')).toBeDefined()
    expect(screen.getByText('View only')).toBeDefined()
    expect(screen.getByText('Can sign in')).toBeDefined()
    expect(screen.getByText('Access paused')).toBeDefined()
    expect(screen.getByText('2 active')).toBeDefined()
    expect(screen.getByText('No active sessions')).toBeDefined()
  })

  test('explains an empty user search result', () => {
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
  })

  test('keeps role values stable while using friendly labels', async () => {
    const user = userEvent.setup()
    const updateUserRole = vi.fn().mockResolvedValue({ ok: true })
    useAdminStore.setState({
      ...originalAdminState,
      users: [mockUser],
      usersTotal: 1,
      usersPage: 1,
      usersLoading: false,
      usersError: null,
      userSearch: '',
      loadUsers: vi.fn(),
      updateUserRole,
    })

    render(<UserManagement />)

    await user.click(screen.getByTitle('Change what this user can do'))
    fireEvent.change(screen.getByRole('combobox', { name: /role for alex operator/i }), {
      target: { value: 'operator' },
    })
    await user.click(screen.getByRole('button', { name: /save role/i }))

    await waitFor(() => expect(updateUserRole).toHaveBeenCalledWith('user-1', 'operator'))
  })

  test('shows the store recovery step when role saving fails', async () => {
    const user = userEvent.setup()
    const updateUserRole = vi.fn().mockResolvedValue({
      ok: false,
      error:
        'You do not have permission to change user access. Ask an owner to update your admin role. Code: 403. Details: owner role required',
    })
    useAdminStore.setState({
      ...originalAdminState,
      users: [mockUser],
      usersTotal: 1,
      usersPage: 1,
      usersLoading: false,
      usersError: null,
      userSearch: '',
      loadUsers: vi.fn(),
      updateUserRole,
    })

    render(<UserManagement />)

    await user.click(screen.getByTitle('Change what this user can do'))
    fireEvent.change(screen.getByRole('combobox', { name: /role for alex operator/i }), {
      target: { value: 'operator' },
    })
    await user.click(screen.getByRole('button', { name: /save role/i }))

    expect(
      await screen.findByText(/You do not have permission to change user access/i)
    ).toBeDefined()
    expect(screen.getByText(/Ask an owner to update your admin role/i)).toBeDefined()
  })
})
