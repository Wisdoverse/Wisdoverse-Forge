import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { AccountSection } from '@app/features/settings/AccountSection'
import { useNavigationStore } from '@app/entities/navigation'
import { useSettingsStore } from '@app/shared/model/settings.store'
import { AuthContext, type AuthContextValue } from '@app/shared/model/auth.context'
import { I18nContext } from '@app/shared/model/i18n.context'
import { ThemeContext } from '@app/shared/model/theme.context'

const changePasswordMock = vi.hoisted(() => vi.fn())
const navigateMock = vi.hoisted(() => vi.fn())

vi.mock('@app/shared/api/legacy', () => ({
  getUserApi: () => ({
    changePassword: changePasswordMock,
  }),
}))

vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => navigateMock,
}))

const loadPreferencesMock = vi.fn().mockResolvedValue(undefined)
const setGettingStartedDismissedMock = vi.fn()
const originalUpdateOrg = useNavigationStore.getState().updateOrg
const originalLoadPreferences = useSettingsStore.getState().loadPreferences
const originalSetGettingStartedDismissed = useSettingsStore.getState().setGettingStartedDismissed

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((res) => {
    resolve = res
  })
  return { promise, resolve }
}

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
  navigateMock.mockReset()
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
        'Enter your current password, then choose a new password with at least 12 characters, one uppercase letter, one lowercase letter, one number, and one symbol.'
      )
    ).toBeDefined()

    fireEvent.change(screen.getByLabelText('Current Password'), {
      target: { value: 'old-password' },
    })
    fireEvent.change(screen.getByLabelText('New Password'), {
      target: { value: 'NewPassword123!' },
    })
    fireEvent.change(screen.getByLabelText('Confirm New Password'), {
      target: { value: 'NewPassword123!' },
    })
    const request = deferred<void>()
    changePasswordMock.mockReturnValueOnce(request.promise)
    fireEvent.click(screen.getByRole('button', { name: /update password/i }))

    expect(screen.getByRole('button', { name: /updating password/i })).toBeDisabled()
    expect(screen.queryByRole('button', { name: /^Saving\.\.\.$/i })).toBeNull()
    request.resolve()

    await waitFor(() =>
      expect(changePasswordMock).toHaveBeenCalledWith('old-password', 'NewPassword123!')
    )
    expect(await screen.findByRole('status')).toHaveTextContent(
      'Password changed. Use the new password the next time you sign in.'
    )
    expect(
      screen.getByText('Password changed. Use the new password the next time you sign in.')
    ).toBeDefined()
  })

  test('stops users from reusing the current password as the new password', async () => {
    renderAccountSection()

    fireEvent.change(screen.getByLabelText('Current Password'), {
      target: { value: 'SamePassword123!' },
    })
    fireEvent.change(screen.getByLabelText('New Password'), {
      target: { value: 'SamePassword123!' },
    })
    fireEvent.change(screen.getByLabelText('Confirm New Password'), {
      target: { value: 'SamePassword123!' },
    })

    expect(
      screen.getByText('Needed: Use a new password that is different from the current password.')
    ).toBeDefined()
    expect(screen.getByRole('button', { name: /update password/i })).toBeDisabled()

    fireEvent.submit(screen.getByLabelText('Current Password').closest('form')!)

    const alert = screen.getByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent(
      'Choose a new password that is different from the current password, then choose Update password again.'
    )
    expect(screen.getByLabelText('New Password')).toHaveFocus()
    expect(changePasswordMock).not.toHaveBeenCalled()
  })

  test('names the retry action when the current password is missing', async () => {
    renderAccountSection()

    fireEvent.change(screen.getByLabelText('New Password'), {
      target: { value: 'NewPassword123!' },
    })
    fireEvent.change(screen.getByLabelText('Confirm New Password'), {
      target: { value: 'NewPassword123!' },
    })
    fireEvent.submit(screen.getByLabelText('Current Password').closest('form')!)

    const alert = screen.getByRole('alert')
    expect(alert).toHaveTextContent(
      'Enter your current password, then choose Update password again.'
    )
    expect(screen.getByLabelText('Current Password')).toHaveFocus()
    expect(changePasswordMock).not.toHaveBeenCalled()
  })

  test('names the retry action when new passwords do not match', async () => {
    renderAccountSection()

    fireEvent.change(screen.getByLabelText('Current Password'), {
      target: { value: 'old-password' },
    })
    fireEvent.change(screen.getByLabelText('New Password'), {
      target: { value: 'NewPassword123!' },
    })
    fireEvent.change(screen.getByLabelText('Confirm New Password'), {
      target: { value: 'OtherPassword123!' },
    })
    fireEvent.submit(screen.getByLabelText('Current Password').closest('form')!)

    const alert = screen.getByRole('alert')
    expect(alert).toHaveTextContent(
      'The two new passwords do not match. Re-enter both new password fields, then choose Update password again.'
    )
    expect(screen.getByLabelText('Confirm New Password')).toHaveFocus()
    expect(changePasswordMock).not.toHaveBeenCalled()
  })

  test('names the Update password button when the new password misses a rule', async () => {
    renderAccountSection()

    fireEvent.change(screen.getByLabelText('Current Password'), {
      target: { value: 'old-password' },
    })
    fireEvent.change(screen.getByLabelText('New Password'), {
      target: { value: 'longpassword' },
    })
    fireEvent.change(screen.getByLabelText('Confirm New Password'), {
      target: { value: 'longpassword' },
    })
    fireEvent.submit(screen.getByLabelText('Current Password').closest('form')!)

    const alert = screen.getByRole('alert')
    expect(alert).toHaveTextContent(
      'Add at least one uppercase letter to the password, then choose Update password again.'
    )
    expect(alert).not.toHaveTextContent('then try again')
    expect(screen.getByLabelText('New Password')).toHaveFocus()
    expect(changePasswordMock).not.toHaveBeenCalled()
  })

  test('shows the same password rules used by sign-up and reset', async () => {
    renderAccountSection()

    fireEvent.change(screen.getByLabelText('Current Password'), {
      target: { value: 'old-password' },
    })
    fireEvent.change(screen.getByLabelText('New Password'), {
      target: { value: 'elevenchars' },
    })
    fireEvent.change(screen.getByLabelText('Confirm New Password'), {
      target: { value: 'elevenchars' },
    })

    expect(
      screen.getByText('Needed: Use at least 12 characters for the new password.')
    ).toBeDefined()
    expect(screen.getByRole('button', { name: /update password/i })).toBeDisabled()

    fireEvent.change(screen.getByLabelText('New Password'), {
      target: { value: 'twelve-chars' },
    })
    fireEvent.change(screen.getByLabelText('Confirm New Password'), {
      target: { value: 'twelve-chars' },
    })

    expect(screen.getByText('Done: Use at least 12 characters for the new password.')).toBeDefined()
    expect(
      screen.getByText('Needed: Add at least one uppercase letter to the password.')
    ).toBeDefined()
    expect(screen.getByRole('button', { name: /update password/i })).toBeDisabled()

    fireEvent.change(screen.getByLabelText('New Password'), {
      target: { value: 'Twelve-chars1' },
    })
    fireEvent.change(screen.getByLabelText('Confirm New Password'), {
      target: { value: 'Twelve-chars1' },
    })

    expect(
      screen.getByText('Done: Add at least one uppercase letter to the password.')
    ).toBeDefined()
    expect(screen.getByText('Done: Add at least one number to the password.')).toBeDefined()
    expect(screen.getByText('Done: Add at least one symbol to the password.')).toBeDefined()
    expect(screen.getByRole('button', { name: /update password/i })).toBeEnabled()
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
    const request = deferred<void>()
    updateOrg.mockReturnValueOnce(request.promise)
    fireEvent.click(screen.getByRole('button', { name: /save team space name/i }))

    expect(screen.getByRole('button', { name: /saving team space name/i })).toBeDisabled()
    expect(screen.queryByRole('button', { name: /^Saving\.\.\.$/i })).toBeNull()
    request.resolve()

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
      screen.getByText(
        'Select a team space from the left menu before changing team space settings.'
      )
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

    expect(screen.getByText('Open Account settings again to load username')).toBeDefined()
    expect(screen.getByText('Open Account settings again to load email')).toBeDefined()
    expect(screen.queryByText('Refresh this page to load username')).toBeNull()
    expect(screen.queryByText('Refresh this page to load email')).toBeNull()
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
      target: { value: 'NewPassword123!' },
    })
    fireEvent.change(screen.getByLabelText('Confirm New Password'), {
      target: { value: 'NewPassword123!' },
    })
    fireEvent.click(screen.getByRole('button', { name: /update password/i }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
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
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert.textContent).toContain('You do not have permission to rename this team space')
    expect(alert.textContent).toContain('Ask an owner or admin to update your team space access')
    expect(alert.textContent).not.toContain('role')
    expect(alert.textContent).toMatch(/^Ask an owner or admin/)
    expect(alert.textContent).not.toContain('organization')
    expect(alert.textContent).not.toContain('API 403')
    expect(alert.textContent).not.toContain('Forbidden')
  })

  test('names the Save team space name button when rename cannot connect', async () => {
    const updateOrg = vi.fn().mockRejectedValue(new Error('Network error'))
    useNavigationStore.setState({ updateOrg })
    renderAccountSection()

    fireEvent.change(screen.getByLabelText('Team Space Name'), {
      target: { value: 'Acme Support' },
    })
    fireEvent.click(screen.getByRole('button', { name: /save team space name/i }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent(
      'Check your connection, then choose Save team space name again. The team space rename did not finish.'
    )
    expect(alert.textContent).not.toContain('rename the team space again')
    expect(alert.textContent).not.toContain('Network error')
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
      target: { value: 'NewPassword123!' },
    })
    fireEvent.change(screen.getByLabelText('Confirm New Password'), {
      target: { value: 'NewPassword123!' },
    })
    fireEvent.click(screen.getByRole('button', { name: /update password/i }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert.textContent).toBe(
      'Re-enter the current password, then choose Update password again. The current password did not match this account.'
    )
    expect(alert.textContent).not.toContain('change your password again')
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
    expect(screen.getByRole('heading', { name: 'Setup checklist' })).toBeDefined()
    expect(screen.queryByText('Onboarding')).toBeNull()
    expect(screen.getByText(/It is hidden from the left menu/i)).toBeDefined()
    expect(screen.queryByText(/If Start is hidden/i)).toBeNull()
    expect(screen.getByText(/choose Reset setup checklist/i)).toBeDefined()
    expect(
      screen.getByText(/Reset setup checklist adds it back to the left menu only/i)
    ).toBeDefined()
    expect(
      screen.getByText(/It is hidden from the left menu, so new sign-ins open Tasks by default/i)
    ).toBeDefined()
    expect(screen.queryByText(/New sign-ins still open Tasks by default/i)).toBeNull()
    expect(screen.getByText(/projects, agents, and tasks stay the same/i)).toBeDefined()
    expect(screen.getByText(/Next step: choose Reset setup checklist/i)).toBeDefined()
    expect(screen.queryByText(/hidden from the left menu right now/i)).toBeNull()
    expect(screen.queryByText(/Reset Start guide/i)).toBeNull()
    expect(screen.queryByText(/Show setup checklist/i)).toBeNull()
    expect(screen.queryByText(/Reset it here/i)).toBeNull()
    expect(screen.queryByText(/sidebar shortcut/i)).toBeNull()
    expect(screen.queryByText(/nothing to restore/i)).toBeNull()

    const restoreButton = screen.getByRole('button', { name: /reset setup checklist/i })
    expect(restoreButton).not.toBeDisabled()
    fireEvent.click(restoreButton)

    await waitFor(() => expect(setGettingStartedDismissedMock).toHaveBeenCalledWith(false))
    expect(await screen.findByRole('status')).toHaveTextContent(
      'Setup checklist was reset and is back in the left menu. Choose Open setup checklist to check setup steps. Your projects, agents, and tasks were not changed.'
    )
    expect(
      screen.getByText(
        'Setup checklist was reset and is back in the left menu. Choose Open setup checklist to check setup steps. Your projects, agents, and tasks were not changed.'
      )
    ).toBeDefined()
    expect(screen.queryByText(/review setup/i)).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: /open setup checklist/i }))
    expect(navigateMock).toHaveBeenCalledWith({ to: '/start' })
  })

  test('lets users restore Start when no preference exists yet', () => {
    useSettingsStore.setState({ preferences: {}, preferencesLoaded: true })

    renderAccountSection()

    expect(screen.getByText(/Next step: choose Reset setup checklist to add it back/i)).toBeDefined()
    expect(screen.queryByText(/hidden from the left menu right now/i)).toBeNull()
    expect(screen.getByRole('button', { name: /reset setup checklist/i })).not.toBeDisabled()
    expect(screen.queryByRole('button', { name: /show setup checklist/i })).toBeNull()
    expect(screen.queryByRole('button', { name: /open setup checklist/i })).toBeNull()
  })

  test('keeps the restore action clear while checklist preference is loading', () => {
    useSettingsStore.setState({ preferences: null, preferencesLoaded: false })

    renderAccountSection()

    expect(screen.getByText(/Wait a moment while Forge checks/i)).toBeDefined()
    expect(
      screen.getByText(/Forge is checking whether the setup checklist is shown/i)
    ).toBeDefined()
    expect(screen.getByText(/Your projects, agents, and tasks stay the same/i)).toBeDefined()
    expect(screen.queryByText(/new sign-ins open Tasks/i)).toBeNull()
    expect(screen.queryByText(/New sign-ins can open the setup checklist/i)).toBeNull()
    expect(screen.queryByText(/already appears in the left menu/i)).toBeNull()
    expect(screen.getByRole('button', { name: /checking/i })).toBeDisabled()
    expect(screen.queryByRole('button', { name: /show setup checklist/i })).toBeNull()
    expect(screen.queryByRole('button', { name: /open setup checklist/i })).toBeNull()
  })

  test('keeps the restore action honest while the guide is already visible', () => {
    useSettingsStore.setState({
      preferences: { gettingStartedDismissed: false },
      preferencesLoaded: true,
    })

    renderAccountSection()

    expect(
      screen.getByText(/It is available now. Choose Open setup checklist to check setup steps/i)
    ).toBeDefined()
    expect(screen.queryByText(/review setup/i)).toBeNull()
    expect(screen.getByText(/It is shown in the left menu/i)).toBeDefined()
    expect(
      screen.getByText(/New sign-ins can open the setup checklist until you hide it again/i)
    ).toBeDefined()
    expect(screen.queryByText(/new sign-ins open Tasks by default/i)).toBeNull()
    expect(screen.queryByText(/already in the left menu/)).toBeNull()
    expect(screen.queryByText(/nothing to restore/i)).toBeNull()
    expect(screen.getByRole('button', { name: /setup checklist already shown/i })).toBeDisabled()
    expect(screen.queryByRole('button', { name: /^reset setup checklist$/i })).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: /open setup checklist/i }))
    expect(navigateMock).toHaveBeenCalledWith({ to: '/start' })
  })

  test('reports a failed restore instead of pretending it worked', async () => {
    setGettingStartedDismissedMock.mockResolvedValue(false)
    useSettingsStore.setState({
      preferences: { gettingStartedDismissed: true },
      preferencesLoaded: true,
    })

    renderAccountSection()

    fireEvent.click(screen.getByRole('button', { name: /reset setup checklist/i }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent(
      'Check your connection, then choose Reset setup checklist again. Forge could not add it back to the left menu.'
    )
    expect(
      screen.queryByText(
        'Setup checklist is back in the left menu. Choose Open setup checklist to check setup steps.'
      )
    ).toBeNull()
    expect(screen.queryByRole('button', { name: /open setup checklist/i })).toBeNull()
  })
})
