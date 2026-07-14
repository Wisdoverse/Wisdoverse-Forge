import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { ProjectsSection } from '@app/pages/settings/ui/ProjectsSection'
import { TeamsSection } from '@app/pages/settings/ui/TeamsSection'

const mocks = vi.hoisted(() => ({
  user: { orgId: 'org-1', role: 'owner' },
  getTeams: vi.fn(),
  createTeam: vi.fn(),
  updateTeam: vi.fn(),
  deleteTeam: vi.fn(),
  getProjects: vi.fn(),
  createProject: vi.fn(),
  updateProject: vi.fn(),
  deleteProject: vi.fn(),
}))

vi.mock('@app/shared/model/auth.context', () => ({
  useAuth: () => ({
    authManager: { logout: vi.fn() },
    user: mocks.user,
    isAuthenticated: true,
    isLoading: false,
  }),
}))

vi.mock('@app/entities/navigation/team', () => ({
  teamApi: {
    getTeams: (...args: unknown[]) => mocks.getTeams(...args),
    createTeam: (...args: unknown[]) => mocks.createTeam(...args),
    updateTeam: (...args: unknown[]) => mocks.updateTeam(...args),
    deleteTeam: (...args: unknown[]) => mocks.deleteTeam(...args),
    getMembers: vi.fn().mockResolvedValue([]),
    addMember: vi.fn(),
    updateMember: vi.fn(),
    removeMember: vi.fn(),
  },
}))

vi.mock('@app/entities/navigation/project', () => ({
  projectApi: {
    getProjects: (...args: unknown[]) => mocks.getProjects(...args),
    createProject: (...args: unknown[]) => mocks.createProject(...args),
    updateProject: (...args: unknown[]) => mocks.updateProject(...args),
    deleteProject: (...args: unknown[]) => mocks.deleteProject(...args),
    getMembers: vi.fn().mockResolvedValue([]),
    addMember: vi.fn(),
    updateMember: vi.fn(),
    removeMember: vi.fn(),
  },
}))

vi.mock('@app/entities/user', () => ({
  userApi: {
    getUsers: vi.fn().mockResolvedValue([]),
  },
}))

vi.mock('@app/features/manage-team', async () => {
  const React = await import('react')

  return {
    CreateTeamForm: ({ onSave }: { onSave: (name: string) => Promise<void> }) => {
      const [formError, setFormError] = React.useState<string | null>(null)

      return (
        <div>
          <div>Team form ready</div>
          {formError && (
            <div role="alert" aria-live="polite">
              {formError}
            </div>
          )}
          <button
            type="button"
            onClick={() =>
              void onSave('Team Alpha').catch((err: unknown) => {
                setFormError(err instanceof Error ? err.message : String(err))
              })
            }
          >
            Save team
          </button>
        </div>
      )
    },
    EditableTeamRow: () => <div>Existing team row</div>,
  }
})

vi.mock('@app/features/manage-project', async () => {
  const React = await import('react')

  return {
    CreateProjectForm: ({
      onSave,
    }: {
      onSave: (name: string, teamId: string, repositoryUrl?: string) => Promise<void>
    }) => {
      const [formError, setFormError] = React.useState<string | null>(null)

      return (
        <div>
          <div>Project form ready</div>
          {formError && (
            <div role="alert" aria-live="polite">
              {formError}
            </div>
          )}
          <button
            type="button"
            onClick={() =>
              void onSave('Project One', 'team-1').catch((err: unknown) => {
                setFormError(err instanceof Error ? err.message : String(err))
              })
            }
          >
            Save project
          </button>
        </div>
      )
    },
    EditableProjectRow: () => <div>Existing project row</div>,
  }
})

vi.mock('@app/features/manage-members', () => ({
  ResourceMembersModal: () => <div>Members dialog</div>,
}))

const teamAlpha = {
  id: 'team-1',
  orgId: 'org-1',
  name: 'Team Alpha',
  slug: 'team-alpha',
  visibility: 'open',
  description: '',
}

describe('workspace settings empty states', () => {
  beforeEach(() => {
    mocks.user = { orgId: 'org-1', role: 'owner' }
    mocks.getTeams.mockResolvedValue([])
    mocks.getProjects.mockResolvedValue([])
    mocks.createTeam.mockResolvedValue({ ...teamAlpha })
    mocks.createProject.mockResolvedValue({
      id: 'project-1',
      teamId: 'team-1',
      name: 'Project One',
      slug: 'project-one',
      color: '#007AFF',
      description: '',
    })
  })

  afterEach(() => {
    cleanup()
    vi.clearAllMocks()
  })

  it('explains team loading before the first setup list appears', async () => {
    mocks.getTeams.mockReturnValue(new Promise(() => undefined))

    render(<TeamsSection />)

    const loading = await screen.findByRole('status', { name: /checking teams/i })
    expect(loading).toHaveTextContent('Checking teams')
    expect(loading).toHaveTextContent(
      'Forge is checking which teams are available in this team space.'
    )
    expect(loading).toHaveTextContent(
      'If this takes more than a moment, open Teams again or ask an owner or admin to check team access.'
    )
    expect(loading).toHaveTextContent('Success looks like a team row or a Create first team step.')
    expect(loading).not.toHaveTextContent('Loading teams')
  })

  it('explains project loading before the first project list appears', async () => {
    mocks.getTeams.mockReturnValue(new Promise(() => undefined))

    render(<ProjectsSection />)

    const loading = await screen.findByRole('status', { name: /checking projects/i })
    expect(loading).toHaveTextContent('Checking projects')
    expect(loading).toHaveTextContent(
      'Forge is checking which projects are available for this team space.'
    )
    expect(loading).toHaveTextContent(
      'If this takes more than a moment, open Projects again or ask an owner or admin to check project access.'
    )
    expect(loading).toHaveTextContent('Success looks like a project row or a New Project step.')
    expect(loading).not.toHaveTextContent('Loading projects')
  })

  it('shows owners a clear first step when no teams exist', async () => {
    render(<TeamsSection />)

    expect(await screen.findByText('Create a team first')).toBeInTheDocument()
    expect(screen.getByText(/Teams keep projects and access together/i)).toBeInTheDocument()
    expect(screen.getByText('Choose Create first team.')).toBeInTheDocument()
    expect(
      screen.getByText('Name it after the people or work area that will share projects.')
    ).toBeInTheDocument()
    expect(screen.getByText('Create the first project in Projects next.')).toBeInTheDocument()
    expect(screen.queryByText(/Teams group projects/i)).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: /new team/i })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /create first team/i })).toBeInTheDocument()
  })

  it('tells non-admin users who can create the first team', async () => {
    mocks.user = { orgId: 'org-1', role: 'member' }

    render(<TeamsSection />)

    expect(
      await screen.findByText('Ask an owner or admin to create the first team')
    ).toBeInTheDocument()
    expect(screen.getByText('Ask an owner or admin to create one team.')).toBeInTheDocument()
    expect(
      screen.getByText('Ask them which team should own the first project.')
    ).toBeInTheDocument()
    expect(screen.getByText('Come back to Projects after the team appears.')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /new team/i })).not.toBeInTheDocument()
  })

  it('guides users to choose a team space before creating teams', () => {
    mocks.user = { role: 'owner' } as typeof mocks.user

    render(<TeamsSection />)

    expect(mocks.getTeams).not.toHaveBeenCalled()
    expect(screen.getByText('Choose a team space first')).toBeInTheDocument()
    expect(screen.getByText('Choose a team space from the account menu.')).toBeInTheDocument()
    expect(screen.getByText('Open Settings, then Teams again.')).toBeInTheDocument()
    expect(screen.getByText('Choose Teams, then create the team.')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /new team/i })).not.toBeInTheDocument()
    expect(screen.queryByText(/Choose an organization first/i)).not.toBeInTheDocument()
  })

  it('confirms team creation and points beginners to Projects', async () => {
    render(<TeamsSection />)

    fireEvent.click(await screen.findByRole('button', { name: /create first team/i }))
    expect(screen.getByText('Team form ready')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: /save team/i }))

    const status = await screen.findByRole('status')
    expect(status).toHaveTextContent('Team "Team Alpha" is ready')
    expect(status).toHaveTextContent(
      'Next: create the first project in Projects. Use Manage people only when this team needs direct access before project work starts.'
    )
    expect(within(status).getByRole('link', { name: /create first project/i })).toHaveAttribute(
      'href',
      '/settings/projects'
    )
    expect(within(status).getByRole('button', { name: /manage people/i })).toBeInTheDocument()
    expect(screen.getByText('Existing team row')).toBeInTheDocument()
  })

  it('shows beginner recovery guidance when teams fail to load', async () => {
    mocks.getTeams.mockRejectedValue(new Error('HTTP 403'))

    render(<TeamsSection />)

    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain(
      'Ask an owner or admin to update your team space access, then open Settings, then Teams again.'
    )
    expect(alert.textContent).not.toContain('workspace access')
    expect(alert.textContent).toMatch(/^Ask an owner or admin/)
    expect(alert.textContent).not.toContain('HTTP 403')
  })

  it('explains that projects need a team before they can be created', async () => {
    render(<ProjectsSection />)

    expect(await screen.findByText('Create a team before adding projects')).toBeInTheDocument()
    const projectsFrame = screen.getByText('Create a team before adding projects').closest('div')!
      .parentElement!.parentElement!
    expect(projectsFrame).toHaveClass('border-y', 'bg-transparent')
    expect(projectsFrame.className).not.toContain('rounded-card')
    expect(projectsFrame.className).not.toMatch(/(^|\s)bg-white(\s|$)/)
    expect(screen.getByText(/Projects live inside teams/i)).toBeInTheDocument()
    expect(screen.getByText('Choose Open Teams.')).toBeInTheDocument()
    expect(
      screen.getByText('Create one team for the people who share this work.')
    ).toBeInTheDocument()
    expect(screen.getByText('Come back to Projects and choose New Project.')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /new project/i })).not.toBeInTheDocument()
    const openTeams = screen.getByRole('link', { name: /open teams/i })
    expect(openTeams).toHaveAttribute('href', '/settings/teams')
  })

  it('guides users to choose a team space before creating projects', () => {
    mocks.user = { role: 'owner' } as typeof mocks.user

    render(<ProjectsSection />)

    expect(mocks.getTeams).not.toHaveBeenCalled()
    expect(screen.getByText('Choose a team space first')).toBeInTheDocument()
    expect(screen.getByText(/Projects belong to teams inside a team space/i)).toBeInTheDocument()
    expect(screen.getByText('Choose a team space from the account menu.')).toBeInTheDocument()
    expect(screen.getByText('Open Settings, then Projects again.')).toBeInTheDocument()
    expect(screen.getByText('Choose Projects, then create the project.')).toBeInTheDocument()
    expect(screen.queryByText(/Choose an organization first/i)).not.toBeInTheDocument()
  })

  it('shows beginner recovery guidance when projects fail to load', async () => {
    mocks.getTeams.mockRejectedValue(new Error('HTTP 500'))

    render(<ProjectsSection />)

    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain(
      'Open Settings, then Projects again. If it still fails, ask an owner or admin to check Projects in Settings.'
    )
    expect(alert.textContent).not.toContain('HTTP 500')
    expect(alert.textContent).not.toContain('team space setup')
    expect(alert.textContent).not.toContain('temporarily unavailable')
    expect(alert.textContent).not.toContain('workspace projects')
  })

  it('turns project loading server failures into a Settings recovery step', async () => {
    mocks.getTeams.mockRejectedValue(new Error('API 503: {"message":"database unavailable"}'))

    render(<ProjectsSection />)

    expect(await screen.findByText(/Open Settings, then Projects again/i)).toBeInTheDocument()
    expect(
      screen.getByText(/ask an owner or admin to check Projects in Settings/i)
    ).toBeInTheDocument()
    expect(screen.queryByText(/team space setup/i)).not.toBeInTheDocument()
    expect(screen.queryByText(/workspace setup/i)).not.toBeInTheDocument()
    expect(screen.queryByText(/database unavailable/i)).not.toBeInTheDocument()
  })

  it('shows project creation when at least one team allows it', async () => {
    mocks.getTeams.mockResolvedValue([{ ...teamAlpha, canCreateProject: true }])
    mocks.getProjects.mockResolvedValue([])

    render(<ProjectsSection />)

    await waitFor(() => expect(mocks.getProjects).toHaveBeenCalledWith('team-1'))
    expect(await screen.findByText('Create your first project')).toBeInTheDocument()
    expect(screen.getByText('Choose New Project.')).toBeInTheDocument()
    expect(screen.getByText('Name it after the app, product, or work area.')).toBeInTheDocument()
    expect(
      screen.getByText('Add a code link only when agents need files right away.')
    ).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /new project/i }))
    expect(screen.getByText('Project form ready')).toBeInTheDocument()
  })

  it('confirms project creation and points beginners to the next setup step', async () => {
    mocks.getTeams.mockResolvedValue([{ ...teamAlpha, canCreateProject: true }])
    mocks.getProjects.mockResolvedValue([])

    render(<ProjectsSection />)

    await waitFor(() => expect(mocks.getProjects).toHaveBeenCalledWith('team-1'))
    fireEvent.click(await screen.findByRole('button', { name: /new project/i }))
    fireEvent.click(screen.getByRole('button', { name: /save project/i }))

    const status = await screen.findByRole('status')
    expect(status).toHaveTextContent('Project "Project One" is ready')
    expect(status).toHaveTextContent(
      'Next: set up a place for new tasks in Agents, then create the first task in Tasks.'
    )
    expect(status).not.toHaveTextContent('task queue')
    expect(within(status).getByRole('link', { name: /set up place/i })).toHaveAttribute(
      'href',
      '/agents'
    )
    expect(within(status).getByRole('link', { name: /create first task/i })).toHaveAttribute(
      'href',
      '/tasks'
    )
    expect(screen.getByText('Existing project row')).toBeInTheDocument()
  })

  it('keeps project creation failures in the form without a duplicate page alert', async () => {
    mocks.getTeams.mockResolvedValue([{ ...teamAlpha, canCreateProject: true }])
    mocks.getProjects.mockResolvedValue([])
    mocks.createProject.mockRejectedValue(new Error('API 503: database unavailable'))

    render(<ProjectsSection />)

    await waitFor(() => expect(mocks.getProjects).toHaveBeenCalledWith('team-1'))
    fireEvent.click(await screen.findByRole('button', { name: /new project/i }))
    fireEvent.click(screen.getByRole('button', { name: /save project/i }))

    const alerts = await screen.findAllByRole('alert')
    expect(alerts).toHaveLength(1)
    expect(alerts[0]).toHaveAttribute('aria-live', 'polite')
    expect(alerts[0]).toHaveTextContent(
      'Open Settings, then Projects again, choose the team, then create this project again. If it still fails, ask an owner or admin to check Projects in Settings.'
    )
    expect(alerts[0]).not.toHaveTextContent('API 503')
    expect(alerts[0]).not.toHaveTextContent('database unavailable')
    expect(screen.getByText('Project form ready')).toBeInTheDocument()
    expect(screen.queryByRole('status')).not.toBeInTheDocument()
  })

  it('explains where to manage project people and access', async () => {
    mocks.getTeams.mockResolvedValue([{ ...teamAlpha, canCreateProject: true }])
    mocks.getProjects.mockResolvedValue([
      {
        id: 'project-1',
        teamId: 'team-1',
        name: 'Website',
        slug: 'website',
        color: '#007AFF',
        description: '',
      },
    ])

    render(<ProjectsSection />)

    await waitFor(() => expect(mocks.getProjects).toHaveBeenCalledWith('team-1'))
    expect(screen.getByText(/1 project across 1 team/i)).toBeInTheDocument()
    expect(
      screen.getByText(/Open Manage people on a project to add people or change access/i)
    ).toBeInTheDocument()
  })

  it('shows who to ask when teams exist but project creation is unavailable', async () => {
    mocks.getTeams.mockResolvedValue([{ ...teamAlpha, canCreateProject: false }])
    mocks.getProjects.mockResolvedValue([])

    render(<ProjectsSection />)

    await waitFor(() => expect(mocks.getProjects).toHaveBeenCalledWith('team-1'))
    expect(
      await screen.findByText('Ask a team admin to let you create projects')
    ).toBeInTheDocument()
    expect(
      screen.getByText('Ask a team admin which team should own this project.')
    ).toBeInTheDocument()
    expect(
      screen.getByText('Ask them to let you create projects in that team.')
    ).toBeInTheDocument()
    expect(screen.getByText('Come back to Projects after access is updated.')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /new project/i })).not.toBeInTheDocument()
  })

  it('shows a beginner recovery step when projects cannot load', async () => {
    mocks.getTeams.mockRejectedValue(new Error('HTTP 403'))

    render(<ProjectsSection />)

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Ask an owner or admin to update your team space access, then open Settings, then Projects again. You do not have access to these project settings right now.'
    )
    expect(screen.queryByText('HTTP 403')).not.toBeInTheDocument()
  })

  it('keeps loaded projects visible when one team project list fails', async () => {
    mocks.getTeams.mockResolvedValue([
      { ...teamAlpha, canCreateProject: true },
      { ...teamAlpha, id: 'team-2', name: 'Team Beta', slug: 'team-beta', canCreateProject: true },
    ])
    mocks.getProjects.mockImplementation((teamId: string) => {
      if (teamId === 'team-2') return Promise.reject(new Error('HTTP 503'))
      return Promise.resolve([
        {
          id: 'project-1',
          teamId: 'team-1',
          name: 'Website',
          slug: 'website',
          color: '#007AFF',
          description: '',
        },
      ])
    })

    render(<ProjectsSection />)

    await waitFor(() => expect(mocks.getProjects).toHaveBeenCalledWith('team-2'))
    expect(screen.getByText('Existing project row')).toBeInTheDocument()
    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Open Settings, then Projects again.')
    expect(alert).toHaveTextContent('Some projects may be missing below.')
    expect(alert).not.toHaveTextContent('HTTP 503')
    expect(screen.queryByText('Create your first project')).not.toBeInTheDocument()
  })
})
