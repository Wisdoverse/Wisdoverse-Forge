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
})
