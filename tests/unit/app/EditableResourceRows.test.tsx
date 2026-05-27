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
    render(<EditableTeamRow team={team} onUpdate={vi.fn()} onDelete={vi.fn()} />)

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
      />
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
