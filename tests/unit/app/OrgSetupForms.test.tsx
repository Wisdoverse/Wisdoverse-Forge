import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { CreateProjectForm } from '@app/features/manage-project/ui/CreateProjectForm'
import { CreateTeamForm } from '@app/features/manage-team/ui/CreateTeamForm'
import type { NavTeam } from '@app/entities/team'

const teams: NavTeam[] = [
  {
    id: 'team-frontend',
    orgId: 'org-1',
    name: 'Frontend',
    slug: 'frontend',
    visibility: 'private',
    description: '',
  },
  {
    id: 'team-ops',
    orgId: 'org-1',
    name: 'Operations',
    slug: 'operations',
    visibility: 'private',
    description: '',
  },
]

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

describe('organization setup forms', () => {
  test('guides team creation before submitting the trimmed team name', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined)

    render(<CreateTeamForm onSave={onSave} onCancel={vi.fn()} saving={false} />)

    expect(screen.getByText('Team setup path')).toBeDefined()
    expect(screen.getByText(/create the team before adding projects/i)).toBeDefined()
    expect(screen.getByRole('button', { name: /create team/i })).not.toBeDisabled()

    fireEvent.change(screen.getByLabelText(/^team name/i), {
      target: { value: ' Platform Ops ' },
    })
    expect(screen.getByText('Slug: platform-ops')).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /create team/i }))

    await waitFor(() => expect(onSave).toHaveBeenCalledWith('Platform Ops'))
  })

  test('guides project creation and submits the selected owning team', async () => {
    const onSave = vi.fn().mockResolvedValue(undefined)

    render(<CreateProjectForm teams={teams} onSave={onSave} onCancel={vi.fn()} saving={false} />)

    expect(screen.getByText('Project setup path')).toBeDefined()
    expect(screen.getByText(/choose the team that owns the work/i)).toBeDefined()
    expect(screen.getByRole('button', { name: /create project/i })).not.toBeDisabled()

    fireEvent.change(screen.getByLabelText(/^project name/i), {
      target: { value: ' Customer Portal ' },
    })
    fireEvent.change(screen.getByLabelText(/^team/i), { target: { value: 'team-ops' } })
    expect(screen.getByText('Slug: customer-portal')).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /create project/i }))

    await waitFor(() => expect(onSave).toHaveBeenCalledWith('Customer Portal', 'team-ops'))
  })

  test('keeps project creation disabled until a team exists', () => {
    render(<CreateProjectForm teams={[]} onSave={vi.fn()} onCancel={vi.fn()} saving={false} />)

    fireEvent.change(screen.getByLabelText(/^project name/i), {
      target: { value: 'Back Office' },
    })

    expect(screen.getByText(/No teams/i)).toBeDefined()
    expect(screen.getByRole('button', { name: /create project/i })).not.toBeDisabled()
  })
})
