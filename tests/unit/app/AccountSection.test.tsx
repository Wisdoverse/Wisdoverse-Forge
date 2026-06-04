import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { AccountSection } from '@app/features/settings/AccountSection'
import { useNavigationStore } from '@app/entities/navigation'
import { AuthContext, type AuthContextValue } from '@app/shared/model/auth.context'
import { I18nContext } from '@app/shared/model/i18n.context'
import { ThemeContext } from '@app/shared/model/theme.context'

const changePasswordMock = vi.hoisted(() => vi.fn())

vi.mock('@app/shared/api/legacy', () => ({
  getUserApi: () => ({
    changePassword: changePasswordMock,
  }),
}))

const originalUpdateOrg = useNavigationStore.getState().updateOrg

function renderAccountSection() {
  const authValue: AuthContextValue = {
    authManager: {} as AuthContextValue['authManager'],
    user: {
      id: 'user-1',
      email: 'operator@example.com',
      username: 'Operator',
      role: 'owner',
      orgId: 'org-1',
    },
    isAuthenticated: true,
    isLoading: false,
  }

  return render(
    <AuthContext.Provider value={authValue}>
      <ThemeContext.Provider
        value={{
          theme: 'light',
          toggleTheme: vi.fn(),
          setTheme: vi.fn(),
        }}
      >
        <I18nContext.Provider value={{ language: 'en', setLanguage: vi.fn() }}>
          <AccountSection />
        </I18nContext.Provider>
      </ThemeContext.Provider>
    </AuthContext.Provider>
  )
}

beforeEach(() => {
  changePasswordMock.mockResolvedValue(undefined)
  useNavigationStore.setState({
    orgs: [
      {
        id: 'org-1',
        name: 'Acme Operations',
        slug: 'acme',
        plan: 'team',
        role: 'owner',
      },
    ],
    selectedOrgId: 'org-1',
    updateOrg: vi.fn().mockResolvedValue(undefined),
  })
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  changePasswordMock.mockReset()
  useNavigationStore.setState({
    orgs: [],
    selectedOrgId: null,
    updateOrg: originalUpdateOrg,
  })
})

describe('AccountSection', () => {
  test('explains the password update path and confirms the next sign-in behavior', async () => {
    renderAccountSection()

    expect(
      screen.getByText(
        'Enter your current password, then choose a new password with at least 8 characters.'
      )
    ).toBeDefined()

    fireEvent.change(screen.getByLabelText('Current Password'), {
      target: { value: 'old-password' },
    })
    fireEvent.change(screen.getByLabelText('New Password'), {
      target: { value: 'new-password' },
    })
    fireEvent.change(screen.getByLabelText('Confirm New Password'), {
      target: { value: 'new-password' },
    })
    fireEvent.click(screen.getByRole('button', { name: /update password/i }))

    await waitFor(() =>
      expect(changePasswordMock).toHaveBeenCalledWith('old-password', 'new-password')
    )
    expect(
      screen.getByText('Password changed. Use the new password the next time you sign in.')
    ).toBeDefined()
  })

  test('makes organization rename consequences and the save action explicit', async () => {
    const updateOrg = vi.fn().mockResolvedValue(undefined)
    useNavigationStore.setState({ updateOrg })

    renderAccountSection()

    expect(
      screen.getByText(
        'This changes the display name only. Projects, teams, and permissions stay where they are.'
      )
    ).toBeDefined()

    fireEvent.change(screen.getByLabelText('Organization Name'), {
      target: { value: 'Acme Support' },
    })
    fireEvent.click(screen.getByRole('button', { name: /save organization name/i }))

    await waitFor(() => expect(updateOrg).toHaveBeenCalledWith('org-1', { name: 'Acme Support' }))
    expect(
      screen.getByText('Organization name updated. Teammates will see the new name in navigation.')
    ).toBeDefined()
  })

  test('guides users when no organization is selected', () => {
    useNavigationStore.setState({ orgs: [], selectedOrgId: null })

    renderAccountSection()

    expect(
      screen.getByText(
        'Select an organization from the sidebar before changing organization settings.'
      )
    ).toBeDefined()
  })

  test('shows sign-in guidance when password update is not authorized', async () => {
    changePasswordMock.mockRejectedValue(new Error('HTTP 401: {"message":"token expired"}'))
    renderAccountSection()

    fireEvent.change(screen.getByLabelText('Current Password'), {
      target: { value: 'old-password' },
    })
    fireEvent.change(screen.getByLabelText('New Password'), {
      target: { value: 'new-password' },
    })
    fireEvent.change(screen.getByLabelText('Confirm New Password'), {
      target: { value: 'new-password' },
    })
    fireEvent.click(screen.getByRole('button', { name: /update password/i }))

    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain('Sign in again')
    expect(alert.textContent).not.toContain('Code: 401.')
    expect(alert.textContent).not.toContain('HTTP 401')
    expect(alert.textContent).not.toContain('token expired')
  })

  test('shows permission guidance when organization rename is denied', async () => {
    const updateOrg = vi.fn().mockRejectedValue(new Error('API 403: Forbidden'))
    useNavigationStore.setState({ updateOrg })
    renderAccountSection()

    fireEvent.change(screen.getByLabelText('Organization Name'), {
      target: { value: 'Acme Support' },
    })
    fireEvent.click(screen.getByRole('button', { name: /save organization name/i }))

    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain('You do not have permission to rename this organization')
    expect(alert.textContent).toContain('Ask an owner or admin')
    expect(alert.textContent).not.toContain('API 403')
    expect(alert.textContent).not.toContain('Forbidden')
  })

  test('shows a password recovery step instead of raw validation details', async () => {
    changePasswordMock.mockRejectedValue(
      Object.assign(new Error('HTTP 422'), {
        statusCode: 422,
        serverError: 'Current password is incorrect.',
      })
    )
    renderAccountSection()

    fireEvent.change(screen.getByLabelText('Current Password'), {
      target: { value: 'old-password' },
    })
    fireEvent.change(screen.getByLabelText('New Password'), {
      target: { value: 'new-password' },
    })
    fireEvent.change(screen.getByLabelText('Confirm New Password'), {
      target: { value: 'new-password' },
    })
    fireEvent.click(screen.getByRole('button', { name: /update password/i }))

    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain('The current password did not match this account')
    expect(alert.textContent).not.toContain('Details:')
    expect(alert.textContent).not.toContain('HTTP 422')
  })
})
