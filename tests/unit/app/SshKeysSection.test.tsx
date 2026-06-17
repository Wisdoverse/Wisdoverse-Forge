import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { SshKeysSection } from '@app/features/settings/SshKeysSection'
import { useSettingsStore } from '@app/shared/model/settings.store'
import type { UserSshKey } from '@app/entities/agent'

const loadSshKeysMock = vi.fn().mockResolvedValue(undefined)
const createSshKeyMock = vi.fn().mockResolvedValue(true)
const deleteSshKeyMock = vi.fn().mockResolvedValue(true)
const originalLoadSshKeys = useSettingsStore.getState().loadSshKeys
const originalCreateSshKey = useSettingsStore.getState().createSshKey
const originalDeleteSshKey = useSettingsStore.getState().deleteSshKey

function sshKey(overrides: Partial<UserSshKey> = {}): UserSshKey {
  return {
    id: 'ssh-key-1',
    label: 'Work laptop',
    fingerprint: 'SHA256:abc123',
    keyType: 'ssh-ed25519',
    createdAt: '2026-05-12T08:00:00.000Z',
    ...overrides,
  }
}

beforeEach(() => {
  loadSshKeysMock.mockResolvedValue(undefined)
  createSshKeyMock.mockResolvedValue(true)
  deleteSshKeyMock.mockResolvedValue(true)
  useSettingsStore.setState({
    sshKeys: [],
    sshKeysLoading: false,
    sshKeysError: null,
    loadSshKeys: loadSshKeysMock,
    createSshKey: createSshKeyMock,
    deleteSshKey: deleteSshKeyMock,
  })
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  useSettingsStore.setState({
    sshKeys: [],
    sshKeysLoading: false,
    sshKeysError: null,
    loadSshKeys: originalLoadSshKeys,
    createSshKey: originalCreateSshKey,
    deleteSshKey: originalDeleteSshKey,
  })
})

describe('SshKeysSection', () => {
  test('guides first-time SSH code access setup and saves only after required fields are filled', async () => {
    render(<SshKeysSection />)

    expect(await screen.findByText('Add access for code links that start with git@')).toBeDefined()
    const emptyState = screen.getByTestId('ssh-access-empty-state')
    expect(within(emptyState).getAllByText(/starts with git@/i).length).toBeGreaterThan(0)
    expect(within(emptyState).getByText(/starts with https:\/\//i)).toBeDefined()
    expect(within(emptyState).getByText(/use Code Repository Access instead/i)).toBeDefined()
    expect(within(emptyState).getByText(/skip this for public projects/i)).toBeDefined()
    expect(
      within(emptyState).getByRole('button', { name: /add SSH code access/i })
    ).toBeDefined()
    expect(within(emptyState).queryByText('No repository access yet')).toBeNull()

    fireEvent.click(
      within(emptyState).getByRole('button', { name: /add SSH code access/i })
    )

    expect(screen.queryByTestId('ssh-access-empty-state')).toBeNull()
    expect(screen.getByText('Add access for code links that start with git@')).toBeDefined()
    expect(screen.getByText('Paste the public line')).toBeDefined()
    expect(screen.getByText(/one-line \.pub key/i)).toBeDefined()
    expect(screen.getAllByText(/starts with ssh-ed25519 or ssh-rsa/i).length).toBeGreaterThan(0)
    expect(screen.getByText('Keep the private key secret')).toBeDefined()
    expect(screen.getAllByText(/BEGIN PRIVATE KEY/i).length).toBeGreaterThan(0)
    expect(
      screen.getByPlaceholderText('ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA... dev@example.com')
    ).toBeDefined()

    const nameInput = screen.getByLabelText(/^name for this access/i)
    const shareableLineInput = screen.getByLabelText(/^public key line/i)
    const form = nameInput.closest('form')
    expect(form).toBeTruthy()

    const saveButton = screen.getByRole('button', { name: /save SSH code access/i })
    expect(saveButton).toBeDisabled()

    fireEvent.submit(form!)
    expect(createSshKeyMock).not.toHaveBeenCalled()
    expect(screen.getByRole('alert')).toHaveTextContent(
      /add a name your team will recognize before saving/i
    )
    expect(nameInput).toHaveFocus()

    fireEvent.change(nameInput, { target: { value: 'Work laptop' } })
    fireEvent.submit(form!)
    expect(createSshKeyMock).not.toHaveBeenCalled()
    expect(screen.getByRole('alert')).toHaveTextContent(/paste the public key line before saving/i)
    expect(shareableLineInput).toHaveFocus()

    fireEvent.change(shareableLineInput, {
      target: { value: 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAexample dev@example.com' },
    })
    expect(saveButton).toBeEnabled()
    fireEvent.click(saveButton)

    await waitFor(() =>
      expect(createSshKeyMock).toHaveBeenCalledWith(
        'Work laptop',
        'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAexample dev@example.com'
      )
    )
    expect(await screen.findByRole('status')).toHaveTextContent(
      'SSH code access saved. Create a small task with a git@ code link to confirm agents can open it.'
    )
    expect(screen.getByRole('status')).toHaveTextContent('come back here and replace this key')
  })

  test('explains the impact before removing SSH code access', async () => {
    const user = userEvent.setup()
    useSettingsStore.setState({ sshKeys: [sshKey()] })

    render(<SshKeysSection />)

    await waitFor(() => expect(loadSshKeysMock).toHaveBeenCalledTimes(1))
    expect(screen.getByText('Safety check')).toBeDefined()
    expect(screen.getByText('Key type')).toBeDefined()
    expect(screen.getByText('Modern key type')).toBeDefined()
    expect(screen.queryByText('Saved key ID')).toBeNull()
    expect(screen.queryByText('Key kind')).toBeNull()

    fireEvent.click(
      screen.getByRole('button', { name: /remove work laptop SSH code access/i })
    )

    expect(deleteSshKeyMock).not.toHaveBeenCalled()
    expect(
      screen.getByText(
        'Removing this access can block agents that use private code links starting with git@.'
      )
    ).toBeDefined()
    expect(screen.getByRole('button', { name: /keep access/i })).toBeDefined()

    await user.click(screen.getByRole('button', { name: /keep access/i }))
    expect(screen.queryByText(/removing this access can block agents/i)).toBeNull()
    expect(deleteSshKeyMock).not.toHaveBeenCalled()

    await user.click(
      screen.getByRole('button', { name: /remove work laptop SSH code access/i })
    )
    const removeNowButton = screen.getByRole('button', {
        name: /confirm removing work laptop SSH code access/i,
    })
    deleteSshKeyMock.mockImplementationOnce(
      () => new Promise((resolve) => setTimeout(() => resolve(true), 20))
    )

    await user.click(removeNowButton)
    expect(removeNowButton).toHaveTextContent('Removing...')
    expect(removeNowButton).toHaveAttribute('aria-busy', 'true')
    expect(screen.getByRole('button', { name: /keep access/i })).toBeDisabled()

    await waitFor(() => expect(deleteSshKeyMock).toHaveBeenCalledWith('ssh-key-1'))
  })

  test('explains missing SSH code access dates instead of showing raw date failures', async () => {
    useSettingsStore.setState({
      sshKeys: [
        sshKey({ createdAt: '' }),
        sshKey({
          id: 'ssh-key-2',
          label: 'Deploy runner',
          fingerprint: 'SHA256:def456',
          createdAt: 'not-a-date',
        }),
      ],
    })

    render(<SshKeysSection />)

    expect(await screen.findByRole('table', { name: /SSH code access/i })).toBeDefined()
    expect(screen.getByText('Refresh SSH code access to load added date')).toBeDefined()
    expect(screen.getByText('Refresh SSH code access to check added date')).toBeDefined()
    expect(screen.queryByText('Invalid Date')).toBeNull()
    expect(screen.queryByText('—')).toBeNull()
  })

  test('shows a beginner recovery step instead of raw SSH key details', async () => {
    useSettingsStore.setState({
      sshKeysError: 'Settings could not save SSH key. Details: invalid public key',
    })

    render(<SshKeysSection />)

    await waitFor(() => expect(loadSshKeysMock).toHaveBeenCalled())
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Paste only the shareable one-line public key that starts with ssh-ed25519 or ssh-rsa, then save again. Do not paste a private key block.'
    )
    expect(screen.queryByText(/Details: invalid public key/i)).toBeNull()
  })

  test('keeps store validation guidance on the save path', async () => {
    useSettingsStore.setState({
      sshKeysError: 'Add a label, paste a valid public SSH key, then save the SSH key again.',
    })

    render(<SshKeysSection />)

    await waitFor(() => expect(loadSshKeysMock).toHaveBeenCalled())
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Add a name for this access, then save again.'
    )
    expect(screen.queryByText(/could not be loaded/i)).toBeNull()
  })
})
