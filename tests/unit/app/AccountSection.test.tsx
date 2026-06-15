import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { AccountSection } from '@app/features/settings/AccountSection'
import { useNavigationStore } from '@app/entities/navigation'
import { useSettingsStore } from '@app/shared/model/settings.store'
import { AuthContext, type AuthContextValue } from '@app/shared/model/auth.context'
import { I18nContext } from '@app/shared/model/i18n.context'
import { ThemeContext } from '@app/shared/model/theme.context'

const changePasswordMock = vi.hoisted(() => vi.fn())

vi.mock('@app/shared/api/legacy', () => ({
  getUserApi: () => ({
    changePassword: changePasswordMock,
  }),
}))

const loadPreferencesMock = vi.fn().mockResolvedValue(undefined)
const setGettingStartedDismissedMock = vi.fn()
const originalUpdateOrg = useNavigationStore.getState().updateOrg
const originalLoadPreferences = useSettingsStore.getState().loadPreferences
const originalSetGettingStartedDismissed = useSettingsStore.getState().setGettingStartedDismissed

function renderAccountSection(
  role = 'owner',
  userOverrides: Partial<NonNullable<AuthContextValue['user']>> = {}
) {
  const authValue: AuthContextValue = {
    authManager: {} as AuthContextValue['authManager'],
    user: {
      id: 'user-1',
      email: 'operator@example.com',
      username: 'Operator',
      role,
      orgId: 'org-1',
      ...userOverrides,
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
  loadPreferencesMock.mockClear()
  setGettingStartedDismissedMock.mockReset().mockResolvedValue(true)
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
  useSettingsStore.setState({
    preferences: {},
    preferencesLoaded: true,
    preferencesLoading: false,
    loadPreferences: loadPreferencesMock,
    setGettingStartedDismissed: setGettingStartedDismissedMock,
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
  useSettingsStore.setState({
    preferences: null,
    preferencesLoaded: false,
    preferencesLoading: false,
    loadPreferences: originalLoadPreferences,
    setGettingStartedDismissed: originalSetGettingStartedDismissed,
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
    expect(await screen.findByRole('status')).toHaveTextContent(
      'Password changed. Use the new password the next time you sign in.'
    )
    expect(
      screen.getByText('Password changed. Use the new password the next time you sign in.')
    ).toBeDefined()
  })

  test('makes team space rename consequences and the save action explicit', async () => {
    const updateOrg = vi.fn().mockResolvedValue(undefined)
    useNavigationStore.setState({ updateOrg })

    renderAccountSection()

    expect(
      screen.getByText(
        'This changes the display name only. Projects, teams, and permissions stay where they are.'
      )
    ).toBeDefined()

    fireEvent.change(screen.getByLabelText('Team Space Name'), {
      target: { value: 'Acme Support' },
    })
    fireEvent.click(screen.getByRole('button', { name: /save team space name/i }))

    await waitFor(() => expect(updateOrg).toHaveBeenCalledWith('org-1', { name: 'Acme Support' }))
    expect(await screen.findByRole('status')).toHaveTextContent(
      'Team space name updated. Teammates will see the new name in navigation.'
    )
    expect(
      screen.getByText('Team space name updated. Teammates will see the new name in navigation.')
    ).toBeDefined()
    expect(screen.queryByText('Organization name updated.')).toBeNull()
  })

  test('guides users when no team space is selected', () => {
    useNavigationStore.setState({ orgs: [], selectedOrgId: null })

    renderAccountSection()

    expect(
      screen.getByText('Select a team space from the sidebar before changing team space settings.')
    ).toBeDefined()
  })

  test('uses a friendly fallback instead of exposing an unknown account role', () => {
    renderAccountSection('billing_admin')

    expect(screen.getByText('Access level')).toBeDefined()
    expect(screen.queryByText('Role')).toBeNull()
    expect(screen.getByText('Check access level')).toBeDefined()
    expect(screen.queryByText('billing_admin')).toBeNull()
    expect(screen.queryByText('billing admin')).toBeNull()
  })

  test('explains missing profile fields without placeholder symbols', () => {
    renderAccountSection('owner', {
      email: ' ',
      username: '',
    })

    expect(screen.getByText('Refresh this page to load username')).toBeDefined()
    expect(screen.getByText('Refresh this page to load email')).toBeDefined()
    expect(screen.queryByText('Username not reported yet')).toBeNull()
    expect(screen.queryByText('Email not reported yet')).toBeNull()
    expect(screen.queryByText('—')).toBeNull()
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
    expect(alert.textContent).toMatch(/^Sign in again/)
    expect(alert.textContent).not.toContain('Code: 401.')
    expect(alert.textContent).not.toContain('HTTP 401')
    expect(alert.textContent).not.toContain('token expired')
  })

  test('shows permission guidance when team space rename is denied', async () => {
    const updateOrg = vi.fn().mockRejectedValue(new Error('API 403: Forbidden'))
    useNavigationStore.setState({ updateOrg })
    renderAccountSection()

    fireEvent.change(screen.getByLabelText('Team Space Name'), {
      target: { value: 'Acme Support' },
    })
    fireEvent.click(screen.getByRole('button', { name: /save team space name/i }))

    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain('You do not have permission to rename this team space')
    expect(alert.textContent).toContain('Ask an owner or admin to update your team space access')
    expect(alert.textContent).not.toContain('role')
    expect(alert.textContent).toMatch(/^Ask an owner or admin/)
    expect(alert.textContent).not.toContain('organization')
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
    expect(alert.textContent).toMatch(/^Re-enter the current password/)
    expect(alert.textContent).not.toContain('Details:')
    expect(alert.textContent).not.toContain('HTTP 422')
  })

  test('restores a hidden Getting Started guide and confirms the result', async () => {
    useSettingsStore.setState({
      preferences: { gettingStartedDismissed: true },
      preferencesLoaded: true,
    })

    renderAccountSection()

    expect(loadPreferencesMock).toHaveBeenCalled()
    expect(screen.getByText('Setup checklist')).toBeDefined()
    expect(screen.getByText(/Skipping Start only hides the sidebar shortcut/i)).toBeDefined()
    expect(screen.getByText(/The setup checklist is hidden right now/i)).toBeDefined()
    expect(screen.queryByText(/Reset Start guide/i)).toBeNull()
    expect(screen.queryByText(/Reset it here/i)).toBeNull()

    const restoreButton = screen.getByRole('button', { name: /show setup checklist/i })
    expect(restoreButton).not.toBeDisabled()
    fireEvent.click(restoreButton)

    await waitFor(() => expect(setGettingStartedDismissedMock).toHaveBeenCalledWith(false))
    expect(await screen.findByRole('status')).toHaveTextContent(
      'The setup checklist is back in the sidebar. Open it when setup needs review.'
    )
    expect(
      screen.getByText(
        'The setup checklist is back in the sidebar. Open it when setup needs review.'
      )
    ).toBeDefined()
    expect(screen.getByRole('link', { name: /open setup checklist/i })).toHaveAttribute(
      'href',
      '/start'
    )
  })

  test('keeps the restore action honest while the guide is already visible', () => {
    useSettingsStore.setState({ preferences: {}, preferencesLoaded: true })

    renderAccountSection()

    expect(screen.getByText(/The setup checklist is already visible in the sidebar/)).toBeDefined()
    expect(screen.getByRole('button', { name: /show setup checklist/i })).toBeDisabled()
    expect(screen.getByRole('link', { name: /open setup checklist/i })).toHaveAttribute(
      'href',
      '/start'
    )
  })

  test('reports a failed restore instead of pretending it worked', async () => {
    setGettingStartedDismissedMock.mockResolvedValue(false)
    useSettingsStore.setState({
      preferences: { gettingStartedDismissed: true },
      preferencesLoaded: true,
    })

    renderAccountSection()

    fireEvent.click(screen.getByRole('button', { name: /show setup checklist/i }))

    expect(
      await screen.findByText(
        'The setup checklist could not be shown. Check your connection, then try again.'
      )
    ).toBeDefined()
    expect(
      screen.queryByText(
        'The setup checklist is back in the sidebar. Open it when setup needs review.'
      )
    ).toBeNull()
    expect(screen.queryByRole('link', { name: /open setup checklist/i })).toBeNull()
  })
})
