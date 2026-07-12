import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { EditableProjectRow } from '@app/features/manage-project/ui/EditableProjectRow'
import { EditableTeamRow } from '@app/features/manage-team/ui/EditableTeamRow'
import type { NavProject } from '@app/entities/navigation/project'
import type { NavTeam } from '@app/entities/navigation/team'

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
  test('labels generated team and project link names without implementation terms', () => {
    render(
      <>
        <EditableTeamRow
          team={team}
          onUpdate={vi.fn().mockResolvedValue(undefined)}
          onDelete={vi.fn()}
        />
        <EditableProjectRow
          project={project}
          teamName="Product"
          onUpdate={vi.fn().mockResolvedValue(undefined)}
          onDelete={vi.fn()}
        />
      </>
    )

    expect(
      screen.getByText(
        /Team link preview:\s*product-team\. Forge creates this automatically from the team name/i
      )
    ).toBeDefined()
    expect(screen.getByText('Invite-only')).toHaveAttribute(
      'title',
      'Only invited people can find this team.'
    )
    expect(screen.queryByText(/^private$/i)).toBeNull()
    expect(
      screen.getByText(
        'Project link preview: website-launch. Forge creates this automatically from the project name'
      )
    ).toBeDefined()
    expect(screen.queryByText(/Team menu link preview/i)).toBeNull()
    expect(screen.queryByText(/Project menu link preview/i)).toBeNull()
    expect(screen.queryByText(/Auto-created/i)).toBeNull()
    expect(screen.queryByText(/Forge uses this in team links/i)).toBeNull()
    expect(screen.queryByText(/Forge uses this in project links/i)).toBeNull()
    expect(screen.queryByText(/Shown at the end of/i)).toBeNull()
    expect(screen.queryByText(/Automatic team name/i)).toBeNull()
    expect(screen.queryByText(/Automatic project name/i)).toBeNull()
    expect(screen.queryByText(/Automatic link name/i)).toBeNull()
    expect(screen.queryByText(/Team short name/i)).toBeNull()
    expect(screen.queryByText(/Project short name/i)).toBeNull()
    expect(screen.queryByText(/Address:\s*product-team/i)).toBeNull()
    expect(screen.queryByText('Address: website-launch')).toBeNull()
    expect(screen.queryByText(/slug/i)).toBeNull()
  })

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

  test('shows beginner guidance when a team save is denied', async () => {
    const onUpdate = vi.fn().mockRejectedValue(new Error('API 403: Forbidden'))

    render(<EditableTeamRow team={team} onUpdate={onUpdate} onDelete={vi.fn()} />)

    fireEvent.click(screen.getByRole('button', { name: 'Edit Product Team' }))
    fireEvent.change(screen.getByLabelText('Team name'), {
      target: { value: 'Product Operators' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Save team' }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert.textContent).toContain('You do not have permission')
    expect(alert.textContent).toContain('Ask an owner or admin')
    expect(alert.textContent).not.toContain('API 403')
    expect(alert.textContent).not.toContain('Forbidden')
  })

  test('guides team editing when the name is empty', () => {
    const onUpdate = vi.fn().mockResolvedValue(undefined)

    render(<EditableTeamRow team={team} onUpdate={onUpdate} onDelete={vi.fn()} />)

    fireEvent.click(screen.getByRole('button', { name: 'Edit Product Team' }))
    fireEvent.change(screen.getByLabelText('Team name'), {
      target: { value: '   ' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Save team' }))

    const alert = screen.getByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent('Enter a team name, then save this team name again.')
    expect(screen.queryByText('Team name is required')).toBeNull()
    expect(onUpdate).not.toHaveBeenCalled()
  })

  test('guides project editing when the name is empty', () => {
    const onUpdate = vi.fn().mockResolvedValue(undefined)

    render(
      <EditableProjectRow
        project={project}
        teamName="Product"
        onUpdate={onUpdate}
        onDelete={vi.fn()}
      />
    )

    fireEvent.click(screen.getByRole('button', { name: 'Edit Website Launch' }))
    fireEvent.change(screen.getByLabelText('Project name'), {
      target: { value: '   ' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Save project' }))

    const alert = screen.getByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent('Enter a project name, then save this project name again.')
    expect(screen.queryByText('Project name is required')).toBeNull()
    expect(onUpdate).not.toHaveBeenCalled()
  })

  test('shows beginner guidance when a project delete is blocked', async () => {
    const onDelete = vi
      .fn()
      .mockRejectedValue(new Error('HTTP 422: {"message":"Move agents first."}'))

    render(
      <EditableProjectRow
        project={project}
        teamName="Product"
        onUpdate={vi.fn().mockResolvedValue(undefined)}
        onDelete={onDelete}
      />
    )

    fireEvent.click(screen.getByRole('button', { name: 'Delete Website Launch' }))
    fireEvent.click(screen.getByRole('button', { name: 'Confirm delete Website Launch' }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert.textContent).toContain('Go to Agents, change or remove agents')
    expect(alert.textContent).not.toContain('Details:')
    expect(alert.textContent).not.toContain('Move agents first.')
    expect(alert.textContent).not.toContain('HTTP 422')
  })
})
