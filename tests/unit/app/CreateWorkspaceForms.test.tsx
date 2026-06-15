import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { CreateProjectForm } from '@app/features/manage-project'
import { CreateTeamForm } from '@app/features/manage-team'
import type { NavTeam } from '@app/entities/team'

const team: NavTeam = {
  id: 'team-1',
  orgId: 'org-1',
  name: 'Platform',
  slug: 'platform',
  visibility: 'private',
  description: '',
}

afterEach(() => {
  cleanup()
})

describe('workspace setup create forms', () => {
  test('keeps team creation actionable and explains a missing name', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined)

    render(<CreateTeamForm onSave={onSave} onCancel={vi.fn()} saving={false} />)

    const status = screen.getByTestId('create-team-status')
    expect(within(status).getByText('Next: name the team')).toBeInTheDocument()
    const createButton = screen.getByRole('button', { name: /create team/i })
    expect(createButton).not.toBeDisabled()

    fireEvent.click(createButton)

    expect(onSave).not.toHaveBeenCalled()
    expect(screen.getByRole('alert')).toHaveTextContent('Enter a team name before creating it.')
    const nameInput = screen.getByLabelText(/team name/i)
    expect(nameInput).toHaveFocus()

    fireEvent.change(nameInput, { target: { value: 'Support Ops' } })

    expect(within(status).getByText('Ready to create team')).toBeInTheDocument()
    fireEvent.click(createButton)

    await waitFor(() => expect(onSave).toHaveBeenCalledWith('Support Ops'))
  })

  test('explains that a project needs a team before it can be created', () => {
    const onSave = vi.fn().mockResolvedValue(undefined)

    render(<CreateProjectForm teams={[]} onSave={onSave} onCancel={vi.fn()} saving={false} />)

    const status = screen.getByTestId('create-project-status')
    expect(within(status).getByText('Next: create a team first')).toBeInTheDocument()
    const createButton = screen.getByRole('button', { name: /create project/i })
    expect(createButton).not.toBeDisabled()

    fireEvent.click(createButton)

    expect(onSave).not.toHaveBeenCalled()
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Create or choose a team before creating this project.'
    )
    expect(screen.getByText('No teams — create a team first')).toHaveFocus()
  })

  test('keeps project creation actionable and focuses the missing name field', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined)

    render(<CreateProjectForm teams={[team]} onSave={onSave} onCancel={vi.fn()} saving={false} />)

    const status = screen.getByTestId('create-project-status')
    expect(within(status).getByText('Next: name the project')).toBeInTheDocument()
    const createButton = screen.getByRole('button', { name: /create project/i })
    expect(createButton).not.toBeDisabled()

    fireEvent.click(createButton)

    expect(onSave).not.toHaveBeenCalled()
    expect(screen.getByRole('alert')).toHaveTextContent('Enter a project name before creating it.')
    const nameInput = screen.getByLabelText(/project name/i)
    expect(nameInput).toHaveFocus()

    fireEvent.change(nameInput, { target: { value: 'Customer Portal' } })

    expect(within(status).getByText('Ready to create project')).toBeInTheDocument()
    fireEvent.click(createButton)

    // Empty optional repo URL → submits WITHOUT a repositoryUrl (third arg undefined).
    await waitFor(() => expect(onSave).toHaveBeenCalledWith('Customer Portal', 'team-1', undefined))
  })

  test('shows the derived read-only workspace path once a name is entered', () => {
    render(<CreateProjectForm teams={[team]} onSave={vi.fn()} onCancel={vi.fn()} saving={false} />)

    expect(screen.queryByText(/\/workspace\//)).not.toBeInTheDocument()

    fireEvent.change(screen.getByLabelText(/project name/i), {
      target: { value: 'My New Repo' },
    })

    // Path is derived (slugified) and shown, not typed by the user.
    expect(screen.getByText('/workspace/my-new-repo')).toBeInTheDocument()
  })

  test('submits a valid https repository URL as the third arg', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined)

    render(<CreateProjectForm teams={[team]} onSave={onSave} onCancel={vi.fn()} saving={false} />)

    fireEvent.change(screen.getByLabelText(/project name/i), {
      target: { value: 'Cloned Project' },
    })
    fireEvent.change(screen.getByLabelText(/git repository url/i), {
      target: { value: 'https://github.com/org/repo.git' },
    })
    fireEvent.click(screen.getByRole('button', { name: /create project/i }))

    await waitFor(() =>
      expect(onSave).toHaveBeenCalledWith(
        'Cloned Project',
        'team-1',
        'https://github.com/org/repo.git'
      )
    )
  })

  test('blocks submit with a visible banner for a non-https repository URL', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined)

    render(<CreateProjectForm teams={[team]} onSave={onSave} onCancel={vi.fn()} saving={false} />)

    fireEvent.change(screen.getByLabelText(/project name/i), {
      target: { value: 'SSH Project' },
    })
    fireEvent.change(screen.getByLabelText(/git repository url/i), {
      target: { value: 'git@github.com:org/repo.git' },
    })
    fireEvent.click(screen.getByRole('button', { name: /create project/i }))

    // No silent dead-click: a visible banner AND no submit.
    await waitFor(() => expect(screen.getByRole('alert')).toBeInTheDocument())
    expect(onSave).not.toHaveBeenCalled()
  })

  test('blocks submit with a visible banner for a credential-bearing URL', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined)

    render(<CreateProjectForm teams={[team]} onSave={onSave} onCancel={vi.fn()} saving={false} />)

    fireEvent.change(screen.getByLabelText(/project name/i), {
      target: { value: 'Token Project' },
    })
    fireEvent.change(screen.getByLabelText(/git repository url/i), {
      target: { value: 'https://user:ghp_secrettoken@github.com/org/repo.git' },
    })
    fireEvent.click(screen.getByRole('button', { name: /create project/i }))

    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent(/remove credentials/i))
    expect(onSave).not.toHaveBeenCalled()
  })

  test('surfaces a server rejection as a banner instead of failing silently', async () => {
    const onSave = vi.fn().mockRejectedValue(new Error('repository_url must be an https URL'))

    render(<CreateProjectForm teams={[team]} onSave={onSave} onCancel={vi.fn()} saving={false} />)

    fireEvent.change(screen.getByLabelText(/project name/i), {
      target: { value: 'Server Rejects' },
    })
    fireEvent.click(screen.getByRole('button', { name: /create project/i }))

    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent('repository_url must be an https URL')
    )
    expect(onSave).toHaveBeenCalled()
  })
})
