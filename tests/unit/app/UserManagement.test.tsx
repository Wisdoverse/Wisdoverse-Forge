import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { UserManagement } from '@app/features/admin/UserManagement'
import { useAdminStore, type AdminUser } from '@app/shared/model/admin.store'

const loadUsersMock = vi.fn().mockResolvedValue(undefined)
const updateUserRoleMock = vi.fn()
const originalLoadUsers = useAdminStore.getState().loadUsers
const originalUpdateUserRole = useAdminStore.getState().updateUserRole

const adminUser: AdminUser = {
  id: 'user-admin',
  email: 'ada@example.com',
  displayName: 'Ada Lovelace',
  role: 'admin',
  status: 'active',
  createdAt: '2026-01-01T00:00:00.000Z',
  lastLoginAt: null,
  sessionsCount: 3,
}

const operatorUser: AdminUser = {
  id: 'user-operator',
  email: 'grace@example.com',
  displayName: 'Grace Hopper',
  role: 'operator',
  status: 'active',
  createdAt: '2026-01-02T00:00:00.000Z',
  lastLoginAt: null,
  sessionsCount: 5,
}

beforeEach(() => {
  loadUsersMock.mockClear()
  updateUserRoleMock.mockReset()
  updateUserRoleMock.mockImplementation(async (id: string, role: string) => {
    useAdminStore.setState((state) => ({
      users: state.users.map((user) => (user.id === id ? { ...user, role } : user)),
    }))
    return true
  })
  useAdminStore.setState({
    users: [adminUser, operatorUser],
    usersTotal: 2,
    usersPage: 1,
    usersLoading: false,
    usersError: null,
    userSearch: '',
    loadUsers: loadUsersMock,
    updateUserRole: updateUserRoleMock,
  })
})

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
  useAdminStore.setState({
    users: [],
    usersTotal: 0,
    usersPage: 1,
    usersLoading: false,
    usersError: null,
    userSearch: '',
    loadUsers: originalLoadUsers,
    updateUserRole: originalUpdateUserRole,
  })
})

describe('UserManagement', () => {
  test('shows searchable users with visible role meanings and edit actions', async () => {
    render(<UserManagement />)

    await waitFor(() => expect(loadUsersMock).toHaveBeenCalledWith(1))
    expect(screen.getByRole('searchbox', { name: /search users by name or email/i })).toBeDefined()
    expect(screen.getByText('2 total users. Review access before changing a role.')).toBeDefined()
    expect(screen.getByText('Can manage users, settings, and system configuration.')).toBeDefined()
    expect(
      screen.getByText('Can run daily workspace operations without changing admin settings.')
    ).toBeDefined()
    expect(screen.getByRole('button', { name: /edit role for ada lovelace/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /edit role for grace hopper/i })).toBeDefined()
  })

  test('guides a role edit before saving and confirms the updated access', async () => {
    render(<UserManagement />)

    fireEvent.click(await screen.findByRole('button', { name: /edit role for ada lovelace/i }))

    const roleSelect = screen.getByRole('combobox', { name: /role for ada lovelace/i })
    const saveButton = screen.getByRole('button', { name: /save role for ada lovelace/i })

    expect(screen.getByText('Choose a different role before saving.')).toBeDefined()
    expect(saveButton).toBeDisabled()

    fireEvent.change(roleSelect, { target: { value: 'viewer' } })

    expect(screen.getByText('Ready to save Viewer access.')).toBeDefined()
    expect(saveButton).toBeEnabled()

    fireEvent.click(saveButton)

    await waitFor(() => expect(updateUserRoleMock).toHaveBeenCalledWith('user-admin', 'viewer'))
    expect(await screen.findByText('Ada Lovelace now has Viewer access.')).toBeDefined()
    expect(screen.getByRole('button', { name: /edit role for ada lovelace/i })).toBeDefined()
  })

  test('keeps the editor open with a clear error when saving fails', async () => {
    updateUserRoleMock.mockResolvedValue(false)

    render(<UserManagement />)

    fireEvent.click(await screen.findByRole('button', { name: /edit role for grace hopper/i }))
    fireEvent.change(screen.getByRole('combobox', { name: /role for grace hopper/i }), {
      target: { value: 'admin' },
    })
    fireEvent.click(screen.getByRole('button', { name: /save role for grace hopper/i }))

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Role could not be saved. Check your permissions and try again.'
    )
    expect(screen.getByRole('combobox', { name: /role for grace hopper/i })).toBeDefined()
  })
})
