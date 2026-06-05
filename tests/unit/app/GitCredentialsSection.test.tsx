import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { GitCredentialsSection } from '@app/features/settings/GitCredentialsSection'
import { useSettingsStore } from '@app/shared/model/settings.store'
import type { GitCredential } from '@app/entities/agent'

const loadGitCredentialsMock = vi.fn().mockResolvedValue(undefined)
const saveGitCredentialMock = vi.fn().mockResolvedValue(true)
const deleteGitCredentialMock = vi.fn().mockResolvedValue(true)
const originalLoadGitCredentials = useSettingsStore.getState().loadGitCredentials
const originalSaveGitCredential = useSettingsStore.getState().saveGitCredential
const originalDeleteGitCredential = useSettingsStore.getState().deleteGitCredential

beforeEach(() => {
  loadGitCredentialsMock.mockClear()
  saveGitCredentialMock.mockClear()
  deleteGitCredentialMock.mockClear()
  useSettingsStore.setState({
    gitCredentials: [],
    gitCredentialsLoading: false,
    gitCredentialsError: null,
    loadGitCredentials: loadGitCredentialsMock,
    saveGitCredential: saveGitCredentialMock,
    deleteGitCredential: deleteGitCredentialMock,
  })
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  useSettingsStore.setState({
    gitCredentials: [],
    gitCredentialsLoading: false,
    gitCredentialsError: null,
    loadGitCredentials: originalLoadGitCredentials,
    saveGitCredential: originalSaveGitCredential,
    deleteGitCredential: originalDeleteGitCredential,
  })
})

describe('GitCredentialsSection', () => {
  test('guides first-time repository access setup before saving a key', async () => {
    render(<GitCredentialsSection />)

    expect(await screen.findByText('No repository access saved yet')).toBeDefined()
    expect(screen.getByText(/use HTTPS addresses/i)).toBeDefined()
    expect(screen.getByText(/use repository SSH keys/i)).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /add repository access/i }))

    expect(screen.getByText('Git access setup path')).toBeDefined()
    expect(screen.getByText('Choose Git service')).toBeDefined()
    expect(screen.getByText('Paste repository access key')).toBeDefined()
    expect(
      screen.getByText(/key from GitHub or GitLab that can reach the repositories/i)
    ).toBeDefined()
    expect(screen.getByText(/leave this empty for github.com or gitlab.com/i)).toBeDefined()
    expect(screen.getByPlaceholderText('e.g. gitlab.example.com')).toBeDefined()

    const saveButton = screen.getByRole('button', { name: /save access/i })
    expect(saveButton).toBeDisabled()

    fireEvent.change(screen.getByLabelText(/^repository access key/i), {
      target: { value: 'ghp_example_token' },
    })
    expect(saveButton).toBeEnabled()
    fireEvent.click(saveButton)

    await waitFor(() =>
      expect(saveGitCredentialMock).toHaveBeenCalledWith('github', 'ghp_example_token', undefined)
    )
  })

  test('shows a beginner recovery step instead of raw git credential details', async () => {
    useSettingsStore.setState({
      gitCredentialsError: 'Settings could not save Git credential. Details: invalid token',
    })

    render(<GitCredentialsSection />)

    await waitFor(() => expect(loadGitCredentialsMock).toHaveBeenCalled())
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Repository access could not be saved. Paste a new repository access key from GitHub or GitLab, then save again.'
    )
    expect(screen.queryByText(/Details: invalid token/i)).toBeNull()
  })
})
