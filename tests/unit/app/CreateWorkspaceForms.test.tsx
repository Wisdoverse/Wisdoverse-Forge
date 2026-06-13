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

    await waitFor(() => expect(onSave).toHaveBeenCalledWith('Customer Portal', 'team-1'))
  })
})
