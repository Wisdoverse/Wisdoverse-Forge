import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
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
  loadSshKeysMock.mockClear()
  createSshKeyMock.mockClear()
  deleteSshKeyMock.mockClear()
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
  test('guides first-time SSH key setup and saves only after required fields are filled', async () => {
    render(<SshKeysSection />)

    expect(await screen.findByText('No repository SSH keys yet')).toBeDefined()
    expect(screen.getByText(/use SSH addresses/i)).toBeDefined()
    expect(screen.getByText(/use repository access tokens for HTTPS/i)).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /add ssh key/i }))

    expect(screen.getByText('SSH key setup path')).toBeDefined()
    expect(screen.getByText('Paste public key')).toBeDefined()
    expect(screen.getAllByText(/starts with ssh-ed25519 or ssh-rsa/i).length).toBeGreaterThan(0)
    expect(screen.getByText('Keep private key private')).toBeDefined()
    expect(screen.getAllByText(/never paste a private key/i).length).toBeGreaterThan(0)

    const saveButton = screen.getByRole('button', { name: /save ssh key/i })
    expect(saveButton).toBeDisabled()

    fireEvent.change(screen.getByLabelText(/^key name/i), { target: { value: 'Work laptop' } })
    fireEvent.change(screen.getByLabelText(/^public key text/i), {
      target: { value: 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAexample user@host' },
    })
    expect(saveButton).toBeEnabled()
    fireEvent.click(saveButton)

    await waitFor(() =>
      expect(createSshKeyMock).toHaveBeenCalledWith(
        'Work laptop',
        'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAexample user@host'
      )
    )
  })

  test('explains the impact before removing an SSH key', async () => {
    useSettingsStore.setState({ sshKeys: [sshKey()] })

    render(<SshKeysSection />)

    await waitFor(() => expect(loadSshKeysMock).toHaveBeenCalledTimes(1))
    fireEvent.click(screen.getByRole('button', { name: /remove work laptop ssh key/i }))

    expect(deleteSshKeyMock).not.toHaveBeenCalled()
    expect(screen.getByText(/can block agents that use private repositories/i)).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /confirm removing work laptop ssh key/i }))

    expect(deleteSshKeyMock).toHaveBeenCalledWith('ssh-key-1')
  })

  test('shows a beginner recovery step instead of raw SSH key details', async () => {
    useSettingsStore.setState({
      sshKeysError: 'Settings could not save SSH key. Details: invalid public key',
    })

    render(<SshKeysSection />)

    await waitFor(() => expect(loadSshKeysMock).toHaveBeenCalled())
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Repository SSH key could not be saved. Paste only the public key line that starts with ssh-ed25519 or ssh-rsa, then save again.'
    )
    expect(screen.queryByText(/Details: invalid public key/i)).toBeNull()
  })
})
