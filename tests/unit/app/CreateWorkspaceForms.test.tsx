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

  test('keeps team creation open and shows a safe recovery step when save fails', async () => {
    const onSave = vi
      .fn()
      .mockRejectedValue(new Error('API 500: database unavailable while creating team'))

    render(<CreateTeamForm onSave={onSave} onCancel={vi.fn()} saving={false} />)

    fireEvent.change(screen.getByLabelText(/team name/i), { target: { value: 'Support Ops' } })
    fireEvent.click(screen.getByRole('button', { name: /create team/i }))

    await waitFor(() => {
      const alert = screen.getByRole('alert')
      expect(alert).toHaveTextContent('Refresh Settings, then create this team again.')
      expect(alert).toHaveTextContent(
        'ask an owner or admin to check Teams and Projects in Settings'
      )
      expect(alert).not.toHaveTextContent('team space setup')
      expect(alert).not.toHaveTextContent('API 500')
      expect(alert).not.toHaveTextContent('database unavailable')
    })
    expect(screen.getByLabelText(/team name/i)).toHaveValue('Support Ops')
    expect(onSave).toHaveBeenCalledWith('Support Ops')
  })

  test('explains that a project needs a team before it can be created', () => {
    const onSave = vi.fn().mockResolvedValue(undefined)

    render(<CreateProjectForm teams={[]} onSave={onSave} onCancel={vi.fn()} saving={false} />)

    expect(
      screen.getByText(/keep one work area's tasks, files, and saved work together/i)
    ).toBeInTheDocument()
    expect(screen.queryByText(/saved work records/i)).not.toBeInTheDocument()
    expect(screen.queryByText(/receive tasks and evidence/i)).toBeNull()
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
    expect(screen.getByTestId('create-project-code-link-status')).toHaveTextContent(
      'No code link added. Create the project now, then add code access later if agents need files.'
    )
    fireEvent.click(createButton)

    // Empty optional repo URL → submits WITHOUT a repositoryUrl (third arg undefined).
    await waitFor(() => expect(onSave).toHaveBeenCalledWith('Customer Portal', 'team-1', undefined))
  })

  test('explains the generated project folder before showing the support folder', () => {
    render(<CreateProjectForm teams={[team]} onSave={vi.fn()} onCancel={vi.fn()} saving={false} />)

    expect(screen.queryByText(/\/workspace\//)).not.toBeInTheDocument()

    fireEvent.change(screen.getByLabelText(/project name/i), {
      target: { value: 'My New Repo' },
    })

    expect(screen.getByText(/Agents will open this project in a folder named/i)).toBeInTheDocument()
    expect(screen.getByText('my-new-repo')).toBeInTheDocument()
    expect(screen.getByText(/You do not need to type this/i)).toBeInTheDocument()
    expect(screen.getByText('Show exact folder for troubleshooting')).toBeInTheDocument()
    expect(screen.queryByText('Show support folder')).toBeNull()
    expect(
      screen.getByText(/Use this only if an owner, admin, or support message asks/i)
    ).toBeInTheDocument()
    expect(screen.getByText('Exact folder: /workspace/my-new-repo')).toBeInTheDocument()
  })

  test('submits a valid https code link as the third arg', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined)

    render(<CreateProjectForm teams={[team]} onSave={onSave} onCancel={vi.fn()} saving={false} />)

    expect(screen.getByText('Code link')).toBeInTheDocument()
    expect(screen.queryByText('Git repository URL')).toBeNull()
    expect(screen.getByPlaceholderText('https://github.com/team/project.git')).toBeInTheDocument()
    expect(screen.queryByPlaceholderText('https://github.com/org/repo.git')).toBeNull()
    expect(screen.getByText(/when you want Forge to copy code now/i)).toBeInTheDocument()
    expect(screen.getAllByText(/Never paste tokens or passwords here/i).length).toBeGreaterThan(0)
    expect(screen.getAllByText(/leave this blank/i).length).toBeGreaterThan(0)
    expect(screen.getByTestId('create-project-code-link-status')).toHaveTextContent(
      'No code link added. Create the project now, then add code access later if agents need files.'
    )
    expect(screen.queryByText(/clone an existing repo/i)).toBeNull()

    fireEvent.change(screen.getByLabelText(/project name/i), {
      target: { value: 'Cloned Project' },
    })
    fireEvent.change(screen.getByLabelText(/code link/i), {
      target: { value: 'https://github.com/team/project.git' },
    })
    expect(screen.getByTestId('create-project-status')).toHaveTextContent(
      'Ready to create project and copy code'
    )
    expect(screen.getByTestId('create-project-code-link-status')).toHaveTextContent(
      'Code copy requested. After creation, watch the project row for Code copy waiting, Copying code, or Code copied. If it needs help, choose Copy code again.'
    )
    fireEvent.click(screen.getByRole('button', { name: /create project/i }))

    await waitFor(() =>
      expect(onSave).toHaveBeenCalledWith(
        'Cloned Project',
        'team-1',
        'https://github.com/team/project.git'
      )
    )
  })

  test('blocks submit with a visible banner for a non-https code link', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined)

    render(<CreateProjectForm teams={[team]} onSave={onSave} onCancel={vi.fn()} saving={false} />)

    fireEvent.change(screen.getByLabelText(/project name/i), {
      target: { value: 'SSH Project' },
    })
    fireEvent.change(screen.getByLabelText(/code link/i), {
      target: { value: 'git@github.com:org/repo.git' },
    })
    fireEvent.click(screen.getByRole('button', { name: /create project/i }))

    // No silent dead-click: a visible banner AND no submit.
    await waitFor(() => expect(screen.getByRole('alert')).toBeInTheDocument())
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Paste a code link that starts with https://'
    )
    expect(screen.getByRole('alert')).toHaveTextContent('leave this blank')
    expect(screen.getByRole('alert')).toHaveTextContent('SSH code access')
    expect(screen.getByRole('alert')).not.toHaveTextContent('SSH keys')
    expect(onSave).not.toHaveBeenCalled()
  })

  test('blocks submit with a visible banner for a credential-bearing URL', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined)

    render(<CreateProjectForm teams={[team]} onSave={onSave} onCancel={vi.fn()} saving={false} />)

    fireEvent.change(screen.getByLabelText(/project name/i), {
      target: { value: 'Token Project' },
    })
    fireEvent.change(screen.getByLabelText(/code link/i), {
      target: { value: 'https://user:ghp_secrettoken@github.com/org/repo.git' },
    })
    fireEvent.click(screen.getByRole('button', { name: /create project/i }))

    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent(/remove account details/i)
    )
    expect(screen.getByRole('alert')).toHaveTextContent('Save code access in Settings')
    expect(onSave).not.toHaveBeenCalled()
  })

  test('surfaces a code link server rejection as a beginner-safe banner', async () => {
    const onSave = vi.fn().mockRejectedValue(new Error('repository_url must be an https URL'))

    render(<CreateProjectForm teams={[team]} onSave={onSave} onCancel={vi.fn()} saving={false} />)

    fireEvent.change(screen.getByLabelText(/project name/i), {
      target: { value: 'Server Rejects' },
    })
    fireEvent.click(screen.getByRole('button', { name: /create project/i }))

    await waitFor(() => {
      const alert = screen.getByRole('alert')
      expect(alert).toHaveTextContent('Paste an https:// code link without account details')
      expect(alert).toHaveTextContent('leave the code link blank')
      expect(alert).toHaveTextContent('add code access in Settings')
      expect(alert).not.toHaveTextContent('repository_url')
    })
    expect(onSave).toHaveBeenCalled()
  })

  test('does not expose raw server details when project creation fails', async () => {
    const onSave = vi
      .fn()
      .mockRejectedValue(new Error('API 500: database unavailable while inserting project'))

    render(<CreateProjectForm teams={[team]} onSave={onSave} onCancel={vi.fn()} saving={false} />)

    fireEvent.change(screen.getByLabelText(/project name/i), {
      target: { value: 'Server Details' },
    })
    fireEvent.click(screen.getByRole('button', { name: /create project/i }))

    await waitFor(() => {
      const alert = screen.getByRole('alert')
      expect(alert).toHaveTextContent('Wait a few minutes, then create this project again.')
      expect(alert).toHaveTextContent('Forge could not create the project right now')
      expect(alert).toHaveTextContent('ask an owner or admin to check Projects in Settings')
      expect(alert).not.toHaveTextContent('project setup')
      expect(alert).not.toHaveTextContent('API 500')
      expect(alert).not.toHaveTextContent('database unavailable')
    })
    expect(onSave).toHaveBeenCalled()
  })

  test('starts project rate-limit failures with the wait step', async () => {
    const onSave = vi.fn().mockRejectedValue(new Error('HTTP 429: too many requests'))

    render(<CreateProjectForm teams={[team]} onSave={onSave} onCancel={vi.fn()} saving={false} />)

    fireEvent.change(screen.getByLabelText(/project name/i), {
      target: { value: 'Busy Project' },
    })
    fireEvent.click(screen.getByRole('button', { name: /create project/i }))

    await waitFor(() => {
      const alert = screen.getByRole('alert')
      expect(alert).toHaveTextContent(
        'Wait a minute, then create this project again. Too many project changes are happening right now.'
      )
      expect(alert).not.toHaveTextContent('HTTP 429')
    })
  })

  test('starts unknown project creation failures with the recovery step', async () => {
    const onSave = vi.fn().mockRejectedValue(new Error('unexpected create failure'))

    render(<CreateProjectForm teams={[team]} onSave={onSave} onCancel={vi.fn()} saving={false} />)

    fireEvent.change(screen.getByLabelText(/project name/i), {
      target: { value: 'Unknown Failure' },
    })
    fireEvent.click(screen.getByRole('button', { name: /create project/i }))

    await waitFor(() => {
      const alert = screen.getByRole('alert')
      expect(alert).toHaveTextContent(
        'Check the project name and team, then create this project again. Forge could not create the project.'
      )
      expect(alert).not.toHaveTextContent('unexpected create failure')
    })
  })
})
