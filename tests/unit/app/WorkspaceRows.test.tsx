import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { EditableProjectRow } from '@app/features/manage-project/ui/EditableProjectRow'
import { EditableTeamRow } from '@app/features/manage-team/ui/EditableTeamRow'
import type { NavProject } from '@app/entities/project'
import type { NavTeam } from '@app/entities/team'

const project: NavProject = {
  id: 'project-1',
  teamId: 'team-1',
  name: 'Website Launch',
  slug: 'website-launch',
  color: '#0066cc',
  description: 'Customer site work',
}

const team: NavTeam = {
  id: 'team-1',
  orgId: 'org-1',
  name: 'Product Team',
  slug: 'product-team',
  visibility: 'private',
  description: 'Builds customer-facing product',
}

afterEach(() => {
  cleanup()
})

describe('workspace management rows', () => {
  test('requires a clear second action before deleting a project', async () => {
    const onDelete = vi.fn().mockResolvedValue(undefined)

    render(
      <EditableProjectRow
        project={project}
        teamName="Product"
        onUpdate={vi.fn().mockResolvedValue(undefined)}
        onDelete={onDelete}
      />
    )

    fireEvent.click(screen.getByRole('button', { name: 'Delete Website Launch' }))

    expect(onDelete).not.toHaveBeenCalled()
    expect(screen.getByRole('button', { name: 'Keep Website Launch' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Confirm delete Website Launch' })).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: 'Keep Website Launch' }))
    expect(screen.queryByRole('button', { name: 'Confirm delete Website Launch' })).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: 'Delete Website Launch' }))
    fireEvent.click(screen.getByRole('button', { name: 'Confirm delete Website Launch' }))

    await waitFor(() => expect(onDelete).toHaveBeenCalledWith(project))
  })

  test('requires a clear second action before deleting a team', async () => {
    const onDelete = vi.fn().mockResolvedValue(undefined)

    render(
      <EditableTeamRow
        team={team}
        onUpdate={vi.fn().mockResolvedValue(undefined)}
        onDelete={onDelete}
      />
    )

    fireEvent.click(screen.getByRole('button', { name: 'Delete Product Team' }))

    expect(onDelete).not.toHaveBeenCalled()
    expect(screen.getByRole('button', { name: 'Keep Product Team' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Confirm delete Product Team' })).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: 'Keep Product Team' }))
    expect(screen.queryByRole('button', { name: 'Confirm delete Product Team' })).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: 'Delete Product Team' }))
    fireEvent.click(screen.getByRole('button', { name: 'Confirm delete Product Team' }))

    await waitFor(() => expect(onDelete).toHaveBeenCalledWith('team-1'))
  })
})
