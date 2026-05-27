import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { GitCredentialsSection } from '@app/features/settings/GitCredentialsSection'
import { useSettingsStore } from '@app/shared/model/settings.store'

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
  test('keeps save available and explains the missing token', async () => {
    render(<GitCredentialsSection />)

    fireEvent.click(screen.getByRole('button', { name: /add token/i }))

    expect(screen.getByTestId('git-credential-form-status')).toHaveTextContent(
      /next: paste access token/i
    )
    const saveButton = screen.getByRole('button', { name: /save credential/i })
    expect(saveButton).toBeEnabled()

    fireEvent.click(saveButton)

    expect(
      screen.getAllByText('Paste an access token before saving this credential.').length
    ).toBeGreaterThan(0)
    expect(saveGitCredentialMock).not.toHaveBeenCalled()
    expect(screen.getByLabelText(/token/i)).toHaveFocus()

    fireEvent.change(screen.getByLabelText(/token/i), { target: { value: 'ghp-example' } })

    expect(screen.getByTestId('git-credential-form-status')).toHaveTextContent(/ready to save/i)
    fireEvent.click(saveButton)

    await waitFor(() =>
      expect(saveGitCredentialMock).toHaveBeenCalledWith('github', 'ghp-example', undefined)
    )
  })

  test('passes a self-hosted GitLab host when provided', async () => {
    render(<GitCredentialsSection />)

    fireEvent.click(screen.getByRole('button', { name: /add token/i }))
    fireEvent.change(screen.getByLabelText(/^provider$/i), { target: { value: 'gitlab' } })
    fireEvent.change(screen.getByLabelText(/token/i), { target: { value: 'glpat-example' } })
    fireEvent.change(screen.getByLabelText(/custom host/i), {
      target: { value: 'gitlab.company.com' },
    })
    fireEvent.click(screen.getByRole('button', { name: /save credential/i }))

    await waitFor(() =>
      expect(saveGitCredentialMock).toHaveBeenCalledWith(
        'gitlab',
        'glpat-example',
        'gitlab.company.com'
      )
    )
  })
})
