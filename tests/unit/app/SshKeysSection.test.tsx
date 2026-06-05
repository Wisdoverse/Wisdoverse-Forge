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
  test('guides first-time repository SSH access setup and saves only after required fields are filled', async () => {
    render(<SshKeysSection />)

    expect(await screen.findByText('No repository SSH access yet')).toBeDefined()
    expect(screen.getAllByText(/addresses.*start.*git@/i).length).toBeGreaterThan(0)
    expect(screen.getByText(/address starts with https:\/\//i)).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /add ssh access/i }))

    expect(screen.getByText('Add repository SSH access')).toBeDefined()
    expect(screen.getByText('Paste the shareable line')).toBeDefined()
    expect(screen.getAllByText(/starts with ssh-ed25519 or ssh-rsa/i).length).toBeGreaterThan(0)
    expect(screen.getByText('Keep the secret key private')).toBeDefined()
    expect(screen.getAllByText(/BEGIN PRIVATE KEY/i).length).toBeGreaterThan(0)
    expect(
      screen.getByPlaceholderText('ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA... dev@example.com')
    ).toBeDefined()

    const nameInput = screen.getByLabelText(/^name for this access/i)
    const shareableLineInput = screen.getByLabelText(/^shareable ssh line/i)
    const form = nameInput.closest('form')
    expect(form).toBeTruthy()

    const saveButton = screen.getByRole('button', { name: /save ssh access/i })
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
    expect(screen.getByRole('alert')).toHaveTextContent(
      /paste the shareable ssh line before saving/i
    )
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
  })

  test('explains the impact before removing repository SSH access', async () => {
    useSettingsStore.setState({ sshKeys: [sshKey()] })

    render(<SshKeysSection />)

    await waitFor(() => expect(loadSshKeysMock).toHaveBeenCalledTimes(1))
    fireEvent.click(
      screen.getByRole('button', { name: /remove work laptop repository ssh access/i })
    )

    expect(deleteSshKeyMock).not.toHaveBeenCalled()
    expect(screen.getByText(/removing this access can block agents/i)).toBeDefined()

    fireEvent.click(
      screen.getByRole('button', { name: /confirm removing work laptop repository ssh access/i })
    )

    expect(deleteSshKeyMock).toHaveBeenCalledWith('ssh-key-1')
  })

  test('shows a beginner recovery step instead of raw SSH key details', async () => {
    useSettingsStore.setState({
      sshKeysError: 'Settings could not save SSH key. Details: invalid public key',
    })

    render(<SshKeysSection />)

    await waitFor(() => expect(loadSshKeysMock).toHaveBeenCalled())
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Repository SSH access could not be saved. Paste only the shareable one-line SSH key that starts with ssh-ed25519 or ssh-rsa, then save again. Do not paste a private key block.'
    )
    expect(screen.queryByText(/Details: invalid public key/i)).toBeNull()
  })
})
