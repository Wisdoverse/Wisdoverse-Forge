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

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((res) => {
    resolve = res
  })
  return { promise, resolve }
}

beforeEach(() => {
  loadGitCredentialsMock.mockResolvedValue(undefined)
  saveGitCredentialMock.mockResolvedValue(true)
  deleteGitCredentialMock.mockResolvedValue(true)
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
  test('explains HTTPS code access loading for first-time setup', () => {
    useSettingsStore.setState({
      gitCredentials: [],
      gitCredentialsLoading: true,
    })

    render(<GitCredentialsSection />)

    const loading = screen.getByRole('status', { name: /checking HTTPS code access/i })
    expect(loading).toHaveTextContent('Checking HTTPS code access')
    expect(loading).toHaveTextContent(
      'Forge is checking which saved keys can open private https:// code links.'
    )
    expect(loading).toHaveTextContent(
      'If this takes more than a moment, open Settings again or ask an owner or admin to check code access.'
    )
    expect(loading).toHaveTextContent('Success looks like saved HTTPS access or a step to add one.')
    expect(loading).not.toHaveTextContent('Loading code access')
  })

  test('guides first-time code access setup before saving a key', async () => {
    const user = userEvent.setup()
    render(<GitCredentialsSection />)

    expect(
      await screen.findByText('Prepare HTTPS code access for private code links')
    ).toBeDefined()
    const emptyState = screen.getByTestId('code-access-empty-state')
    expect(
      within(emptyState).getByText(
        /private GitHub or GitLab code links that start with https:\/\//i
      )
    ).toBeDefined()
    expect(within(emptyState).getByText(/links that start with https:\/\//i)).toBeDefined()
    expect(within(emptyState).getAllByText(/use SSH code access instead/i).length).toBeGreaterThan(
      0
    )
    expect(within(emptyState).queryByText(/github\.com\/team\/project\.git/i)).toBeNull()
    expect(within(emptyState).getByText('Pick the code website')).toBeDefined()
    expect(within(emptyState).getByText('Choose GitHub or GitLab.')).toBeDefined()
    expect(within(emptyState).getByText('Copy a code access key')).toBeDefined()
    expect(
      within(emptyState).getByText(
        'This is different from the code link. Create a read-only key for the code projects agents need, then copy it once.'
      )
    ).toBeDefined()
    expect(
      within(emptyState).getByText('Leave the address empty for github.com or gitlab.com')
    ).toBeDefined()
    expect(
      within(emptyState).getByText(/private code website like gitlab\.example\.com/i)
    ).toBeDefined()
    expect(within(emptyState).queryByText('No repository access saved yet')).toBeNull()
    expect(within(emptyState).queryByText(/default cloud address/i)).toBeNull()

    fireEvent.click(within(emptyState).getByRole('button', { name: /add HTTPS code access/i }))

    expect(screen.getByText('Add code access')).toBeDefined()
    expect(screen.getByText('Pick the code website')).toBeDefined()
    expect(screen.getByText('Copy a code access key')).toBeDefined()
    expect(
      screen.getByText(
        'This is different from the code link. Create a read-only key for the code projects agents need, then copy it once.'
      )
    ).toBeDefined()
    expect(screen.getByText('Leave the address empty for github.com or gitlab.com')).toBeDefined()
    expect(
      screen.getByText(/Only fill it in when your company uses a private code website/i)
    ).toBeDefined()
    expect(screen.queryByText('Use the normal website by default')).toBeNull()
    expect(screen.queryByText(/leave address blank for cloud/i)).toBeNull()
    expect(screen.getByText(/next: paste the code access key/i)).toBeDefined()
    expect(
      screen.getByText(
        /This is not the project code link\. Open the code website, create a read-only key for the code projects agents need, then paste it below\./i
      )
    ).toBeDefined()
    expect(screen.queryByText('Paste the access token')).toBeNull()
    expect(screen.queryByText(/look for a personal access token/i)).toBeNull()
    expect(screen.queryByText(/paste the key from GitHub or GitLab/i)).toBeNull()
    expect(screen.queryByLabelText(/^repository access token/i)).toBeNull()
    expect(screen.getByLabelText(/^code website/i)).toBeDefined()
    expect(screen.getByText('Choose the website where this code lives.')).toBeDefined()
    expect(screen.queryByText('Git service')).toBeNull()
    expect(screen.queryByText(/owns the repository/i)).toBeNull()
    expect(
      screen.getByText(
        /This is not the project code link\. Paste the code access key from GitHub or GitLab\. If that page says personal access token, use that value here\./i
      )
    ).toBeDefined()
    expect(screen.queryByText(/Paste the key you copied from GitHub or GitLab/i)).toBeNull()
    expect(screen.queryByText(/^Paste the code link/i)).toBeNull()
    expect(screen.getByText(/Never paste your website password/i)).toBeDefined()
    expect(screen.queryByText(/Never paste your GitHub or GitLab password/i)).toBeNull()
    expect(screen.getByText(/Forge hides the key after saving/i)).toBeDefined()
    expect(screen.getByText(/leave this empty if you use github.com or gitlab.com/i)).toBeDefined()
    expect(screen.getByLabelText(/^private code website address/i)).toBeDefined()
    expect(screen.queryByLabelText(/^company code website address/i)).toBeNull()
    expect(screen.getByText(/For github.com or gitlab.com, leave this empty/i)).toBeDefined()
    expect(screen.queryByText(/company-hosted Git service/i)).toBeNull()
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
    const alert = screen.getByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent(
      /paste the code access key from GitHub or GitLab before saving/i
    )
    expect(tokenInput).toHaveFocus()
    expect(tokenInput).toHaveAttribute(
      'aria-describedby',
      'git-credential-token-intro git-credential-token-safety git-credential-token-error'
    )

    await user.type(tokenInput, 'ghp_example_token')
    expect(saveButton).toBeEnabled()
    const request = deferred<boolean>()
    saveGitCredentialMock.mockReturnValueOnce(request.promise)
    await user.click(saveButton)

    expect(screen.getByRole('button', { name: /saving HTTPS code access/i })).toBeDisabled()
    expect(screen.queryByRole('button', { name: /^Saving\.\.\.$/i })).toBeNull()
    request.resolve(true)

    await waitFor(() =>
      expect(saveGitCredentialMock).toHaveBeenCalledWith('github', 'ghp_example_token', undefined)
    )
    expect(await screen.findByRole('status')).toHaveTextContent(
      'Code access saved. Create a small task with an https:// private code link to confirm agents can open it.'
    )
    expect(screen.getByRole('status')).toHaveTextContent('If agents cannot open the code')
    expect(screen.getByRole('status')).not.toHaveTextContent('private repository link')
    expect(screen.getByRole('status')).not.toHaveTextContent('read the repository')
    expect(screen.getByRole('status')).toHaveTextContent('come back here and replace this key')
  })

  test('uses a clear confirmation label before removing code access', async () => {
    const user = userEvent.setup()
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
    expect(screen.getByRole('button', { name: /keep access/i })).toBeDefined()
    expect(
      screen.getByText(/removing this access can stop agents from opening private code on GitHub/i)
    ).toBeDefined()

    await user.click(screen.getByRole('button', { name: /keep access/i }))
    expect(screen.queryByText(/removing this access can stop agents/i)).toBeNull()
    expect(deleteGitCredentialMock).not.toHaveBeenCalled()

    await user.click(screen.getByRole('button', { name: /remove github code access/i }))
    const removeNowButton = screen.getByRole('button', {
      name: /confirm removing github code access/i,
    })
    deleteGitCredentialMock.mockImplementationOnce(
      () => new Promise((resolve) => setTimeout(() => resolve(true), 20))
    )

    await user.click(removeNowButton)
    expect(removeNowButton).toHaveTextContent('Removing...')
    expect(removeNowButton).toHaveAttribute('aria-busy', 'true')
    expect(screen.getByRole('button', { name: /keep access/i })).toBeDisabled()

    await waitFor(() => {
      expect(deleteGitCredentialMock).toHaveBeenCalledWith('git-1')
    })
  })

  test('stops a project code link from being saved as the HTTPS code access key', async () => {
    const user = userEvent.setup()
    render(<GitCredentialsSection />)

    const emptyState = await screen.findByTestId('code-access-empty-state')
    await user.click(within(emptyState).getByRole('button', { name: /add HTTPS code access/i }))

    const tokenInput = screen.getByLabelText(/^code access key/i)
    await user.type(tokenInput, 'https://github.com/team/project.git')
    await user.click(screen.getByRole('button', { name: /save code access/i }))

    expect(saveGitCredentialMock).not.toHaveBeenCalled()
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Paste the code access key, not the project code link. Add the project code link when you create the project or task.'
    )
    expect(tokenInput).toHaveFocus()
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
    expect(screen.getByText('Code website')).toBeDefined()
    expect(screen.getByText('Website address')).toBeDefined()
    expect(screen.queryByText('Git service')).toBeNull()
    expect(screen.getByText('github.com')).toBeDefined()
    expect(screen.getByText('Open HTTPS code access again to load added date')).toBeDefined()
    expect(screen.getByText('Open HTTPS code access again to check added date')).toBeDefined()
    expect(screen.queryByText('Default cloud address')).toBeNull()
    expect(screen.queryByText('Git address')).toBeNull()
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

  test('maps raw code access errors before rendering them', async () => {
    useSettingsStore.setState({
      gitCredentialsError: 'HTTP 422: Details: invalid token for provider github',
    })

    render(<GitCredentialsSection />)

    await waitFor(() => expect(loadGitCredentialsMock).toHaveBeenCalled())
    const alert = screen.getByRole('alert')
    expect(alert).toHaveTextContent(
      'Paste a new code access key from GitHub or GitLab, then save again.'
    )
    expect(alert).not.toHaveTextContent('Details')
    expect(alert).not.toHaveTextContent('invalid token')
    expect(alert).not.toHaveTextContent('HTTP')
    expect(alert).not.toHaveTextContent('API')
    expect(alert).not.toHaveTextContent('provider github')
  })
})
