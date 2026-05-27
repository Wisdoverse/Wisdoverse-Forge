import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { SshKeysSection } from '@app/features/settings'
import { useSettingsStore } from '@app/shared/model/settings.store'

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
  test('guides first-time users through the SSH key fields before saving', async () => {
    render(<SshKeysSection />)

    fireEvent.click(screen.getByRole('button', { name: /add key/i }))

    const status = screen.getByTestId('ssh-key-form-status')
    expect(within(status).getByText('Next: Name the SSH Key')).toBeInTheDocument()
    const saveButton = screen.getByRole('button', { name: /add ssh key/i })
    expect(saveButton).not.toBeDisabled()

    fireEvent.click(saveButton)

    expect(createSshKeyMock).not.toHaveBeenCalled()
    expect(screen.getByRole('alert')).toHaveTextContent('Name this SSH key before saving it.')
    const labelInput = screen.getByLabelText(/label/i)
    expect(labelInput).toHaveFocus()

    fireEvent.change(labelInput, { target: { value: ' Laptop Key ' } })
    expect(within(status).getByText('Next: Paste the Public Key')).toBeInTheDocument()

    fireEvent.click(saveButton)

    expect(createSshKeyMock).not.toHaveBeenCalled()
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Paste the public SSH key before saving it.'
    )
    const publicKeyInput = screen.getByLabelText(/public key/i)
    expect(publicKeyInput).toHaveFocus()

    fireEvent.change(publicKeyInput, {
      target: { value: ' ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAEXAMPLE user@host ' },
    })
    expect(within(status).getByText('Ready to Add SSH Key')).toBeInTheDocument()

    fireEvent.click(saveButton)

    await waitFor(() =>
      expect(createSshKeyMock).toHaveBeenCalledWith(
        'Laptop Key',
        'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAEXAMPLE user@host'
      )
    )
  })
})
