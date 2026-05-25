import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { EditableProjectRow } from '@app/features/manage-project/ui/EditableProjectRow'
import { EditableTeamRow } from '@app/features/manage-team/ui/EditableTeamRow'
import type { NavProject } from '@app/entities/project'
import type { NavTeam } from '@app/entities/team'

afterEach(cleanup)

const team: NavTeam = {
  id: 'team-1',
  orgId: 'org-1',
  name: 'Platform Team',
  slug: 'platform',
  visibility: 'private',
  description: 'Core operators',
}

const project: NavProject = {
  id: 'project-1',
  teamId: team.id,
  workspaceId: 'workspace-1',
  name: 'Launch Project',
  slug: 'launch',
  color: '#0066cc',
  description: 'Release work',
}

describe('settings delete guidance', () => {
  test('explains team deletion before calling the destructive action', async () => {
    const user = userEvent.setup()
    const onDelete = vi.fn().mockResolvedValue(undefined)

    render(
      <EditableTeamRow
        team={team}
        onUpdate={vi.fn().mockResolvedValue(undefined)}
        onDelete={onDelete}
      />
    )

    await user.click(screen.getByRole('button', { name: 'Delete Platform Team' }))

    expect(onDelete).not.toHaveBeenCalled()
    expect(screen.getByText('Delete team')).toBeInTheDocument()
    expect(
      screen.getByText(/projects in this team will also disappear from the sidebar/i)
    ).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Confirm delete Platform Team' }))

    expect(onDelete).toHaveBeenCalledWith(team.id)
  })

  test('explains project deletion before calling the destructive action', async () => {
    const user = userEvent.setup()
    const onDelete = vi.fn().mockResolvedValue(undefined)

    render(
      <EditableProjectRow
        project={project}
        teamName={team.name}
        onUpdate={vi.fn().mockResolvedValue(undefined)}
        onDelete={onDelete}
      />
    )

    await user.click(screen.getByRole('button', { name: 'Delete Launch Project' }))

    expect(onDelete).not.toHaveBeenCalled()
    expect(screen.getByText('Delete project')).toBeInTheDocument()
    expect(
      screen.getByText(/agents assigned here will be moved out of this project/i)
    ).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Confirm delete Launch Project' }))

    expect(onDelete).toHaveBeenCalledWith(project)
  })
})
