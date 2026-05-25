import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { SshKeysSection } from '@app/features/settings/SshKeysSection'
import { useSettingsStore } from '@app/shared/model/settings.store'
import type { UserSshKey } from '@app/shared/api/legacy/AgentAPI'

const loadSshKeysMock = vi.fn().mockResolvedValue(undefined)
const createSshKeyMock = vi.fn().mockResolvedValue(true)
const deleteSshKeyMock = vi.fn().mockResolvedValue(true)
const originalLoadSshKeys = useSettingsStore.getState().loadSshKeys
const originalCreateSshKey = useSettingsStore.getState().createSshKey
const originalDeleteSshKey = useSettingsStore.getState().deleteSshKey

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
  test('explains when repository SSH keys are needed', async () => {
    render(<SshKeysSection />)

    await waitFor(() => expect(loadSshKeysMock).toHaveBeenCalled())
    expect(screen.getByRole('heading', { name: /repository ssh keys/i })).toBeDefined()
    expect(screen.getByText(/access private repositories without a password/i)).toBeDefined()
    expect(screen.getByRole('button', { name: /add ssh key/i })).toBeDefined()
    expect(screen.getByText('No repository SSH keys yet')).toBeDefined()
    expect(screen.getByText(/private repository work that uses ssh access/i)).toBeDefined()
  })

  test('labels saved keys by name, fingerprint, type, date, and removal action', async () => {
    const savedKey: UserSshKey = {
      id: 'ssh-key-1',
      label: 'GitHub deploy key',
      fingerprint: 'SHA256:examplefingerprint',
      keyType: 'ed25519',
      publicKey: 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA example@host',
      createdAt: '2026-05-01T12:00:00.000Z',
      updatedAt: '2026-05-01T12:00:00.000Z',
    }
    useSettingsStore.setState({ sshKeys: [savedKey] })

    render(<SshKeysSection />)

    const table = await screen.findByRole('table', { name: /repository ssh keys/i })
    expect(within(table).getByText('Key name')).toBeDefined()
    expect(within(table).getByText('Fingerprint')).toBeDefined()
    expect(within(table).getByText('Key type')).toBeDefined()
    expect(within(table).getByText('Added on')).toBeDefined()
    expect(within(table).getByText('GitHub deploy key')).toBeDefined()
    expect(within(table).getByText('SHA256:examplefingerprint')).toBeDefined()

    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: /remove github deploy key ssh key/i }))
    await user.click(
      screen.getByRole('button', { name: /confirm removing github deploy key ssh key/i })
    )

    expect(deleteSshKeyMock).toHaveBeenCalledWith('ssh-key-1')
  })

  test('guides users to save a named public key and warns against private keys', async () => {
    const user = userEvent.setup()
    render(<SshKeysSection />)

    await user.click(await screen.findByRole('button', { name: /add ssh key/i }))

    expect(screen.getByLabelText(/key name/i)).toBeDefined()
    expect(screen.getByText(/where this key is used/i)).toBeDefined()
    expect(screen.getByLabelText(/public key text/i)).toBeDefined()
    expect(screen.getByText(/never paste a private key/i)).toBeDefined()

    await user.type(screen.getByLabelText(/key name/i), 'GitHub deploy key')
    await user.type(
      screen.getByLabelText(/public key text/i),
      'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA example@host'
    )
    await user.click(screen.getByRole('button', { name: /save ssh key/i }))

    await waitFor(() =>
      expect(createSshKeyMock).toHaveBeenCalledWith(
        'GitHub deploy key',
        'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA example@host'
      )
    )
  })
})
