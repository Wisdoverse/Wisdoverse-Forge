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
  test('guides first-time code access setup before saving a key', async () => {
    render(<GitCredentialsSection />)

    expect(await screen.findByText('Give agents access to private code')).toBeDefined()
    expect(screen.getByText(/links that start with https:\/\//i)).toBeDefined()
    expect(screen.getByText(/use SSH access instead/i)).toBeDefined()
    expect(screen.queryByText('No repository access saved yet')).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /add code access/i }))

    expect(screen.getByText('Add code access')).toBeDefined()
    expect(screen.getByText('Choose where your code lives')).toBeDefined()
    expect(screen.getByText('Create a code access key')).toBeDefined()
    expect(screen.getByText(/allow it to read the code agents need/i)).toBeDefined()
    expect(screen.getByText(/next: create a code access key/i)).toBeDefined()
    expect(screen.queryByText('Paste the access token')).toBeNull()
    expect(screen.queryByText(/look for a personal access token/i)).toBeNull()
    expect(screen.queryByLabelText(/^repository access token/i)).toBeNull()
    expect(screen.getByText(/sites may call it a personal access token/i)).toBeDefined()
    expect(screen.getByText(/do not paste your GitHub or GitLab password/i)).toBeDefined()
    expect(screen.getByText(/leave this empty if you use github.com or gitlab.com/i)).toBeDefined()
    expect(screen.getByPlaceholderText('e.g. gitlab.example.com')).toBeDefined()

    const tokenInput = screen.getByLabelText(/^code access key/i)
    const form = tokenInput.closest('form')
    expect(form).toBeTruthy()
    const saveButton = screen.getByRole('button', { name: /save code access/i })
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
      /paste the code access key from GitHub or GitLab before saving/i
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

  test('uses a clear confirmation label before removing code access', async () => {
    const credential: GitCredential = {
      id: 'git-1',
      provider: 'github',
      host: null,
      createdAt: '2026-06-01T00:00:00Z',
      updatedAt: '2026-06-01T00:00:00Z',
    }
    useSettingsStore.setState({ gitCredentials: [credential] })

    render(<GitCredentialsSection />)

    const removeButton = await screen.findByRole('button', {
      name: /remove github code access/i,
    })
    expect(removeButton).toHaveTextContent('Remove')

    fireEvent.click(removeButton)

    expect(deleteGitCredentialMock).not.toHaveBeenCalled()
    const confirmButton = screen.getByRole('button', {
      name: /confirm removing github code access/i,
    })
    expect(confirmButton).toHaveTextContent('Remove access now')
    expect(confirmButton).not.toHaveTextContent('Remove access?')

    fireEvent.click(confirmButton)

    await waitFor(() => {
      expect(deleteGitCredentialMock).toHaveBeenCalledWith('git-1')
    })
  })

  test('explains missing code access dates instead of showing raw date failures', async () => {
    const credentials: GitCredential[] = [
      {
        id: 'git-1',
        provider: 'github',
        host: null,
        createdAt: '',
        updatedAt: '2026-06-01T00:00:00Z',
      },
      {
        id: 'git-2',
        provider: 'gitlab',
        host: 'gitlab.example.com',
        createdAt: 'not-a-date',
        updatedAt: '2026-06-01T00:00:00Z',
      },
    ]
    useSettingsStore.setState({ gitCredentials: credentials })

    render(<GitCredentialsSection />)

    expect(await screen.findByRole('table', { name: /^code access$/i })).toBeDefined()
    expect(screen.getByText('Refresh code access to load added date')).toBeDefined()
    expect(screen.getByText('Refresh code access to check added date')).toBeDefined()
    expect(screen.queryByText('Invalid Date')).toBeNull()
    expect(screen.queryByText('—')).toBeNull()
  })

  test('shows a beginner recovery step instead of raw git credential details', async () => {
    useSettingsStore.setState({
      gitCredentialsError: 'Paste the code access key from GitHub or GitLab, then save again.',
    })

    render(<GitCredentialsSection />)

    await waitFor(() => expect(loadGitCredentialsMock).toHaveBeenCalled())
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Paste the code access key from GitHub or GitLab, then save again.'
    )
    expect(screen.queryByText(/Details: invalid token/i)).toBeNull()
  })
})
