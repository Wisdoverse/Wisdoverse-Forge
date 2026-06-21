import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { EditableProjectRow } from '@app/features/manage-project'
import { EditableTeamRow } from '@app/features/manage-team'
import type { NavProject } from '@app/entities/project'
import type { NavTeam } from '@app/entities/team'

afterEach(cleanup)

const team: NavTeam = {
  id: 'team-1',
  orgId: 'org-1',
  name: 'Platform',
  slug: 'platform',
  visibility: 'open',
  description: '',
}

const project: NavProject = {
  id: 'project-1',
  teamId: 'team-1',
  name: 'Web App',
  slug: 'web-app',
  color: '#0066cc',
  description: '',
}

describe('Editable resource rows', () => {
  test('uses purpose-focused team edit and delete copy', () => {
    render(
      <EditableTeamRow
        team={team}
        onUpdate={vi.fn()}
        onDelete={vi.fn()}
        onManageMembers={vi.fn()}
      />
    )

    expect(
      screen.getByText(
        'Team link preview: platform. Forge creates this automatically from the team name'
      )
    ).toBeDefined()
    expect(screen.getByText('Open to team space')).toHaveAttribute(
      'title',
      'People in this team space can find this team.'
    )
    expect(screen.queryByText(/^open$/i)).toBeNull()
    expect(screen.queryByText(/Forge uses this in team links/i)).toBeNull()
    expect(screen.queryByText(/Automatic team name/i)).toBeNull()
    expect(screen.queryByTitle('Members')).toBeNull()
    expect(screen.queryByTitle('Edit')).toBeNull()
    expect(screen.queryByTitle('Delete')).toBeNull()
    expect(
      screen.getByRole('button', { name: 'Manage people and access for Platform' })
    ).toHaveAttribute('title', 'Manage people and access')
    expect(screen.getByRole('button', { name: 'Edit Platform' })).toHaveAttribute(
      'title',
      'Rename team'
    )
    expect(screen.getByRole('button', { name: 'Delete Platform' })).toHaveAttribute(
      'title',
      'Delete team'
    )

    fireEvent.click(screen.getByRole('button', { name: 'Edit Platform' }))
    expect(screen.getByPlaceholderText('What this team owns')).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: 'Cancel team edit' }))
    fireEvent.click(screen.getByRole('button', { name: 'Delete Platform' }))
    expect(screen.getByRole('button', { name: 'Confirm delete Platform' })).toHaveTextContent(
      'Delete team'
    )
  })

  test('uses purpose-focused project edit and delete copy', () => {
    render(
      <EditableProjectRow
        project={project}
        teamName="Platform"
        onUpdate={vi.fn()}
        onDelete={vi.fn()}
        onManageMembers={vi.fn()}
      />
    )

    expect(
      screen.getByText(
        'Project link preview: web-app. Forge creates this automatically from the project name'
      )
    ).toBeDefined()
    expect(screen.queryByText(/Auto-created/i)).toBeNull()
    expect(screen.queryByText(/Forge uses this in project links/i)).toBeNull()
    expect(screen.queryByText(/Automatic project name/i)).toBeNull()
    expect(screen.queryByTitle('Members')).toBeNull()
    expect(screen.queryByTitle('Edit')).toBeNull()
    expect(screen.queryByTitle('Delete')).toBeNull()
    expect(
      screen.getByRole('button', { name: 'Manage people and access for Web App' })
    ).toHaveAttribute('title', 'Manage people and access')
    expect(screen.getByRole('button', { name: 'Edit Web App' })).toHaveAttribute(
      'title',
      'Rename project'
    )
    expect(screen.getByRole('button', { name: 'Delete Web App' })).toHaveAttribute(
      'title',
      'Delete project'
    )

    fireEvent.click(screen.getByRole('button', { name: 'Edit Web App' }))
    expect(screen.getByPlaceholderText('What work belongs here')).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: 'Cancel project edit' }))
    fireEvent.click(screen.getByRole('button', { name: 'Delete Web App' }))
    expect(screen.getByRole('button', { name: 'Confirm delete Web App' })).toHaveTextContent(
      'Delete project'
    )
  })
})
