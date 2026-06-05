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
    expect(screen.getByText(/use repository SSH access/i)).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /add repository access/i }))

    expect(screen.getByText('Add repository access')).toBeDefined()
    expect(screen.getByText('Choose where code lives')).toBeDefined()
    expect(screen.getByText('Create a repository access key')).toBeDefined()
    expect(screen.getByText(/look for a personal access token/i)).toBeDefined()
    expect(screen.getByText(/next: create a repository access key/i)).toBeDefined()
    expect(screen.queryByText('Paste the access token')).toBeNull()
    expect(screen.queryByLabelText(/^repository access token/i)).toBeNull()
    expect(screen.getByText(/sites may call it a personal access token/i)).toBeDefined()
    expect(screen.getByText(/do not paste your GitHub or GitLab password/i)).toBeDefined()
    expect(screen.getByText(/leave this empty if you use github.com or gitlab.com/i)).toBeDefined()
    expect(screen.getByPlaceholderText('e.g. gitlab.example.com')).toBeDefined()

    const tokenInput = screen.getByLabelText(/^repository access key/i)
    const form = tokenInput.closest('form')
    expect(form).toBeTruthy()
    const saveButton = screen.getByRole('button', { name: /save repository access/i })
    expect(saveButton).toBeDisabled()

    expect(tokenInput).toHaveAttribute(
      'aria-describedby',
      'git-credential-token-intro git-credential-token-safety'
    )
    expect(document.querySelectorAll('[id="git-credential-token-intro"]')).toHaveLength(1)
    expect(document.querySelectorAll('[id="git-credential-token-safety"]')).toHaveLength(1)

    fireEvent.submit(form!)
    expect(saveGitCredentialMock).not.toHaveBeenCalled()
    expect(screen.getByRole('alert')).toHaveTextContent(
      /paste the repository access key from GitHub or GitLab before saving/i
    )
    expect(tokenInput).toHaveFocus()
    expect(tokenInput).toHaveAttribute(
      'aria-describedby',
      'git-credential-token-intro git-credential-token-safety git-credential-token-error'
    )

    fireEvent.change(tokenInput, {
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
