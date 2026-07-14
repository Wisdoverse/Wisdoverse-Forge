import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { SshKeysSection } from '@app/features/settings/SshKeysSection'
import { useSettingsStore } from '@app/entities/settings'
import type { UserSshKey } from '@app/entities/agent'

const loadSshKeysMock = vi.fn().mockResolvedValue(undefined)
const createSshKeyMock = vi.fn().mockResolvedValue(true)
const deleteSshKeyMock = vi.fn().mockResolvedValue(true)
const originalLoadSshKeys = useSettingsStore.getState().loadSshKeys
const originalCreateSshKey = useSettingsStore.getState().createSshKey
const originalDeleteSshKey = useSettingsStore.getState().deleteSshKey

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((res) => {
    resolve = res
  })
  return { promise, resolve }
}

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
  test('explains SSH-link code access loading for first-time setup', () => {
    useSettingsStore.setState({
      sshKeys: [],
      sshKeysLoading: true,
    })

    render(<SshKeysSection />)

    const loading = screen.getByRole('status', {
      name: /checking private git@ code links/i,
    })
    expect(loading).toHaveTextContent('Checking private git@ code links')
    expect(loading).toHaveTextContent(
      'Forge is checking whether saved access can open code links that start with git@.'
    )
    expect(loading).toHaveTextContent(
      'If this takes more than a moment, open Settings again or ask an owner or admin to check code access.'
    )
    expect(loading).toHaveTextContent('Success looks like saved access or a clear step to add it.')
    expect(loading).not.toHaveTextContent('Loading SSH code access')
  })

  test('guides first-time SSH-link code access setup and saves only after required fields are filled', async () => {
    render(<SshKeysSection />)

    expect(await screen.findByText('Prepare code access for private SSH links')).toBeDefined()
    const emptyState = screen.getByTestId('ssh-access-empty-state')
    expect(within(emptyState).getByText(/starts with https:\/\//i)).toBeDefined()
    expect(within(emptyState).getByText(/use code access for HTTPS links instead/i)).toBeDefined()
    expect(within(emptyState).getByText(/skip this for public projects/i)).toBeDefined()
    expect(within(emptyState).getByText('Name the computer or team')).toBeDefined()
    expect(
      within(emptyState).getAllByText(/Use a name people will recognize/i).length
    ).toBeGreaterThan(0)
    expect(within(emptyState).getByText('Paste the safe public key line')).toBeDefined()
    expect(within(emptyState).getByText(/public key from the \.pub file/i)).toBeDefined()
    expect(within(emptyState).getByText('Never paste the private key')).toBeDefined()
    expect(within(emptyState).getByText(/copy the \.pub line instead/i)).toBeDefined()
    expect(
      within(emptyState).getByRole('button', { name: /add code access for SSH links/i })
    ).toBeDefined()
    expect(within(emptyState).queryByText('No repository access yet')).toBeNull()

    fireEvent.click(
      within(emptyState).getByRole('button', { name: /add code access for SSH links/i })
    )

    expect(screen.queryByTestId('ssh-access-empty-state')).toBeNull()
    expect(screen.getByText('Add code access for SSH links')).toBeDefined()
    expect(screen.getByText('Name the computer or team')).toBeDefined()
    expect(screen.getAllByText(/Use a name people will recognize/i).length).toBeGreaterThan(0)
    expect(screen.getByText('Paste the safe public key line')).toBeDefined()
    expect(screen.getByText(/one-line public key from the \.pub file/i)).toBeDefined()
    expect(screen.queryByText('Paste the public line')).toBeNull()
    expect(screen.getAllByText(/starts with ssh-ed25519 or ssh-rsa/i).length).toBeGreaterThan(0)
    expect(screen.getByText('Never paste the private key')).toBeDefined()
    expect(screen.getAllByText(/BEGIN PRIVATE KEY/i).length).toBeGreaterThan(0)
    expect(screen.getAllByText(/copy the \.pub line instead/i).length).toBeGreaterThan(0)
    expect(
      screen.getByPlaceholderText('ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA... dev@example.com')
    ).toBeDefined()

    const nameInput = screen.getByLabelText(/^name for this access/i)
    const safePublicLineInput = screen.getByLabelText(/^safe public key line/i)
    const form = nameInput.closest('form')
    expect(form).toBeTruthy()

    const saveButton = screen.getByRole('button', { name: /save code access/i })
    expect(saveButton).toBeDisabled()

    fireEvent.submit(form!)
    expect(createSshKeyMock).not.toHaveBeenCalled()
    const missingNameAlert = screen.getByRole('alert')
    expect(missingNameAlert).toHaveAttribute('aria-live', 'polite')
    expect(missingNameAlert).toHaveTextContent(/add a name your team will recognize before saving/i)
    expect(nameInput).toHaveFocus()

    fireEvent.change(nameInput, { target: { value: 'Work laptop' } })
    fireEvent.submit(form!)
    expect(createSshKeyMock).not.toHaveBeenCalled()
    const missingPublicKeyAlert = screen.getByRole('alert')
    expect(missingPublicKeyAlert).toHaveAttribute('aria-live', 'polite')
    expect(missingPublicKeyAlert).toHaveTextContent(/paste the safe public key line before saving/i)
    expect(missingPublicKeyAlert).toHaveTextContent(/safe/i)
    expect(safePublicLineInput).toHaveFocus()

    fireEvent.change(safePublicLineInput, {
      target: {
        value:
          '-----BEGIN OPENSSH PRIVATE KEY-----\nprivate-key-body\n-----END OPENSSH PRIVATE KEY-----',
      },
    })
    expect(saveButton).toBeEnabled()
    fireEvent.click(saveButton)
    expect(createSshKeyMock).not.toHaveBeenCalled()
    const privateKeyAlert = screen.getByRole('alert')
    expect(privateKeyAlert).toHaveAttribute('aria-live', 'polite')
    expect(privateKeyAlert).toHaveTextContent(/looks like a private key/i)
    expect(privateKeyAlert).toHaveTextContent(/copy the one-line \.pub public key/i)
    expect(safePublicLineInput).toHaveFocus()

    fireEvent.change(safePublicLineInput, {
      target: { value: 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAexample dev@example.com' },
    })
    expect(saveButton).toBeEnabled()
    const request = deferred<boolean>()
    createSshKeyMock.mockReturnValueOnce(request.promise)
    fireEvent.click(saveButton)

    expect(screen.getByRole('button', { name: /saving code access for SSH links/i })).toBeDisabled()
    expect(screen.queryByRole('button', { name: /^Saving\.\.\.$/i })).toBeNull()
    request.resolve(true)

    await waitFor(() =>
      expect(createSshKeyMock).toHaveBeenCalledWith(
        'Work laptop',
        'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAexample dev@example.com'
      )
    )
    expect(await screen.findByRole('status')).toHaveTextContent(
      'Code access for SSH links saved. Create a small task with a git@ private code link to confirm agents can open it.'
    )
    expect(screen.getByRole('status')).toHaveTextContent('If agents cannot open the code')
    expect(screen.getByRole('status')).toHaveTextContent('come back here and replace this key')
    expect(screen.getByRole('status')).not.toHaveTextContent('repository')
  })

  test('explains the impact before removing code access for SSH links', async () => {
    const user = userEvent.setup()
    useSettingsStore.setState({
      sshKeys: [sshKey(), sshKey({ id: 'ssh-key-2', label: 'Build runner', keyType: 'ssh-rsa' })],
    })

    render(<SshKeysSection />)

    await waitFor(() => expect(loadSshKeysMock).toHaveBeenCalledTimes(1))
    expect(screen.getByText('How to recognize it')).toBeDefined()
    expect(screen.queryByText('Saved key check text')).toBeNull()
    expect(screen.queryByText('Saved key check code')).toBeNull()
    expect(screen.getByText('Can Forge use it?')).toBeDefined()
    expect(screen.queryByText('Accepted by Forge')).toBeNull()
    expect(screen.getByText('Best for new access')).toBeDefined()
    expect(screen.getByText('Older, still works')).toBeDefined()
    expect(screen.queryByText('Recommended for new access')).toBeNull()
    expect(screen.queryByText('Works, but older')).toBeNull()
    expect(screen.queryByText('Safety check')).toBeNull()
    expect(screen.queryByText('Key type')).toBeNull()
    expect(screen.queryByText('Modern key type')).toBeNull()
    expect(screen.queryByText('Saved key ID')).toBeNull()
    expect(screen.queryByText('Key kind')).toBeNull()

    fireEvent.click(
      screen.getByRole('button', { name: /remove work laptop code access for SSH links/i })
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
      screen.getByRole('button', { name: /remove work laptop code access for SSH links/i })
    )
    const removeNowButton = screen.getByRole('button', {
      name: /confirm removing work laptop code access for SSH links/i,
    })
    let resolveDelete: (removed: boolean) => void = () => {}
    deleteSshKeyMock.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveDelete = resolve
        })
    )

    await user.click(removeNowButton)
    await waitFor(() => expect(removeNowButton).toHaveTextContent('Removing...'))
    expect(removeNowButton).toHaveAttribute('aria-busy', 'true')
    expect(screen.getByRole('button', { name: /keep access/i })).toBeDisabled()

    resolveDelete(true)
    await waitFor(() => expect(deleteSshKeyMock).toHaveBeenCalledWith('ssh-key-1'))
  })

  test('stops multiple public key lines before saving code access for SSH links', async () => {
    const user = userEvent.setup()
    render(<SshKeysSection />)

    const emptyState = await screen.findByTestId('ssh-access-empty-state')
    await user.click(
      within(emptyState).getByRole('button', { name: /add code access for SSH links/i })
    )

    const nameInput = screen.getByLabelText(/^name for this access/i)
    const safePublicLineInput = screen.getByLabelText(/^safe public key line/i)
    await user.type(nameInput, 'Work laptop')
    fireEvent.change(safePublicLineInput, {
      target: {
        value:
          'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAfirst dev@example.com\nssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAsecond dev@example.com',
      },
    })

    await user.click(screen.getByRole('button', { name: /save code access/i }))

    expect(createSshKeyMock).not.toHaveBeenCalled()
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Paste one safe public key line from the .pub file. Remove any extra lines, then save again.'
    )
    expect(safePublicLineInput).toHaveFocus()
  })

  test('explains missing SSH-link code access dates instead of showing raw date failures', async () => {
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

    const table = await screen.findByRole('table', { name: /code access for SSH links/i })
    expect(table).toBeDefined()
    const tableFrame = table.parentElement
    expect(tableFrame).toHaveClass('border-y', 'bg-transparent')
    expect(tableFrame?.className).not.toContain('rounded-card')
    expect(tableFrame?.className).not.toMatch(/(^|\s)bg-white(\s|$)/)
    expect(screen.getByRole('columnheader', { name: 'How to recognize it' })).toBeDefined()
    expect(screen.queryByText('Saved key check text')).toBeNull()
    expect(
      screen.getByText('Open code access for SSH links again to load added date')
    ).toBeDefined()
    expect(
      screen.getByText('Open code access for SSH links again to check added date')
    ).toBeDefined()
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
      'Paste only the safe one-line public key from the .pub file, then save again. Do not paste a private key block.'
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
