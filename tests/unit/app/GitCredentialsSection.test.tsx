import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { GitCredentialsSection } from '@app/features/settings/GitCredentialsSection'
import { useSettingsStore } from '@app/shared/model/settings.store'
import type { GitCredential } from '@app/shared/api/legacy/AgentAPI'

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
  test('explains the empty repository token setup in user-facing language', async () => {
    render(<GitCredentialsSection />)

    await waitFor(() => expect(loadGitCredentialsMock).toHaveBeenCalled())
    expect(screen.getByRole('heading', { name: /repository access tokens/i })).toBeDefined()
    expect(screen.getByText(/connect github or gitlab/i)).toBeDefined()
    expect(screen.getByRole('button', { name: /add repository token/i })).toBeDefined()
    expect(screen.getByText('No repository access tokens yet')).toBeDefined()
    expect(
      screen.getByText(/before assigning work that needs private repository access/i)
    ).toBeDefined()
  })

  test('labels saved tokens by provider, address, date, and removal action', async () => {
    const savedCredential: GitCredential = {
      id: 'credential-1',
      provider: 'github',
      host: null,
      createdAt: '2026-05-01T12:00:00.000Z',
      updatedAt: '2026-05-01T12:00:00.000Z',
    }
    useSettingsStore.setState({ gitCredentials: [savedCredential] })

    render(<GitCredentialsSection />)

    const table = await screen.findByRole('table', { name: /repository access tokens/i })
    expect(within(table).getByText('Git provider')).toBeDefined()
    expect(within(table).getByText('Address')).toBeDefined()
    expect(within(table).getByText('Added on')).toBeDefined()
    expect(within(table).getByText('GitHub')).toBeDefined()
    expect(within(table).getByText('Default cloud address')).toBeDefined()

    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: /remove github repository token/i }))
    await user.click(
      screen.getByRole('button', { name: /confirm removing github repository token/i })
    )

    expect(deleteGitCredentialMock).toHaveBeenCalledWith('credential-1')
  })

  test('collects provider, token, and optional self-hosted address before saving', async () => {
    const user = userEvent.setup()
    render(<GitCredentialsSection />)

    await user.click(await screen.findByRole('button', { name: /add repository token/i }))

    expect(screen.getByLabelText(/git provider/i)).toBeDefined()
    expect(screen.getByText(/choose where the repository is hosted/i)).toBeDefined()
    expect(screen.getByLabelText(/access token/i)).toBeDefined()
    expect(screen.getByText(/it will not be shown again after saving/i)).toBeDefined()
    expect(screen.getByLabelText(/self-hosted git address/i)).toBeDefined()
    expect(screen.getByText(/leave blank for github.com or gitlab.com/i)).toBeDefined()

    await user.selectOptions(screen.getByLabelText(/git provider/i), 'gitlab')
    await user.type(screen.getByLabelText(/access token/i), 'glpat-example')
    await user.type(screen.getByLabelText(/self-hosted git address/i), 'gitlab.company.com')
    await user.click(screen.getByRole('button', { name: /save token/i }))

    await waitFor(() =>
      expect(saveGitCredentialMock).toHaveBeenCalledWith(
        'gitlab',
        'glpat-example',
        'gitlab.company.com'
      )
    )
  })
})
