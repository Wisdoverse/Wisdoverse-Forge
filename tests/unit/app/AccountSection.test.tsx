import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { AccountSection } from '@app/features/settings/AccountSection'
import { useNavigationStore } from '@app/entities/navigation'

const userApiMock = vi.hoisted(() => ({
  changePassword: vi.fn(),
}))

vi.mock('@app/shared/api/legacy', () => ({
  getUserApi: () => userApiMock,
}))

vi.mock('@app/shared/model/auth.context', () => ({
  useAuth: () => ({
    user: {
      username: 'dev',
      email: 'dev@example.com',
      role: 'owner',
    },
  }),
}))

vi.mock('@app/shared/model/theme.context', () => ({
  useTheme: () => ({
    theme: 'light',
    toggleTheme: vi.fn(),
  }),
}))

vi.mock('@app/shared/model/i18n.context', () => ({
  useI18n: () => ({
    language: 'en',
    setLanguage: vi.fn(),
  }),
}))

beforeEach(() => {
  userApiMock.changePassword.mockResolvedValue(undefined)
  useNavigationStore.setState({
    orgs: [
      { id: 'org-1', name: 'Workspace Org', slug: 'workspace-org', plan: 'pro', role: 'owner' },
    ],
    selectedOrgId: 'org-1',
  })
})

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
  useNavigationStore.getState().reset()
})

describe('AccountSection password change', () => {
  test('shows the next missing password step before the submit button is enabled', () => {
    render(<AccountSection />)

    const currentPassword = screen.getByLabelText(/^current password$/i)
    const newPassword = screen.getByLabelText(/^new password$/i)
    const confirmPassword = screen.getByLabelText(/^confirm new password$/i)
    const submit = screen.getByRole('button', { name: /change password/i })

    expect(screen.getByText('Next: Enter your current password.')).toBeDefined()
    expect(submit).toBeDisabled()

    fireEvent.change(currentPassword, { target: { value: 'old-password' } })

    expect(screen.getByText('Next: Use at least 8 characters for the new password.')).toBeDefined()
    expect(submit).toBeDisabled()

    fireEvent.change(newPassword, { target: { value: 'new-password' } })

    expect(screen.getByText('Next: Confirm the new password.')).toBeDefined()
    expect(submit).toBeDisabled()

    fireEvent.change(confirmPassword, { target: { value: 'different-password' } })

    expect(screen.getByText('Next: Make the confirmation match the new password.')).toBeDefined()
    expect(submit).toBeDisabled()

    fireEvent.change(confirmPassword, { target: { value: 'new-password' } })

    expect(screen.getByText('Ready to update your password.')).toBeDefined()
    expect(submit).toBeEnabled()
  })

  test('submits the password change and clears the form after success', async () => {
    render(<AccountSection />)

    const currentPassword = screen.getByLabelText(/^current password$/i) as HTMLInputElement
    const newPassword = screen.getByLabelText(/^new password$/i) as HTMLInputElement
    const confirmPassword = screen.getByLabelText(/^confirm new password$/i) as HTMLInputElement

    fireEvent.change(currentPassword, { target: { value: 'old-password' } })
    fireEvent.change(newPassword, { target: { value: 'new-password' } })
    fireEvent.change(confirmPassword, { target: { value: 'new-password' } })
    fireEvent.click(screen.getByRole('button', { name: /change password/i }))

    await waitFor(() =>
      expect(userApiMock.changePassword).toHaveBeenCalledWith('old-password', 'new-password')
    )
    expect(await screen.findByText('Password changed successfully')).toBeDefined()
    expect(currentPassword.value).toBe('')
    expect(newPassword.value).toBe('')
    expect(confirmPassword.value).toBe('')
  })
})
