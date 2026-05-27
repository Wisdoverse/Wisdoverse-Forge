import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
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

vi.mock('@app/entities/team', () => ({
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

vi.mock('@app/entities/project', () => ({
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

vi.mock('@app/features/manage-team', () => ({
  CreateTeamForm: () => <div>Team form ready</div>,
  EditableTeamRow: () => <div>Existing team row</div>,
}))

vi.mock('@app/features/manage-project', () => ({
  CreateProjectForm: () => <div>Project form ready</div>,
  EditableProjectRow: () => <div>Existing project row</div>,
}))

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

  it('shows owners a clear first step when no teams exist', async () => {
    render(<TeamsSection />)

    expect(await screen.findByText('Create a team first')).toBeInTheDocument()
    expect(
      screen.getByText(/Teams group projects and decide who can manage work/i)
    ).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /new team/i })).toBeInTheDocument()
  })

  it('tells non-admin users who can create the first team', async () => {
    mocks.user = { orgId: 'org-1', role: 'member' }

    render(<TeamsSection />)

    expect(
      await screen.findByText('Ask an owner or admin to create the first team')
    ).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /new team/i })).not.toBeInTheDocument()
  })

  it('explains that projects need a team before they can be created', async () => {
    render(<ProjectsSection />)

    expect(await screen.findByText('Create a team before adding projects')).toBeInTheDocument()
    expect(screen.getByText(/Projects live inside teams/i)).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /new project/i })).not.toBeInTheDocument()
  })

  it('shows project creation when at least one team allows it', async () => {
    mocks.getTeams.mockResolvedValue([{ ...teamAlpha, canCreateProject: true }])
    mocks.getProjects.mockResolvedValue([])

    render(<ProjectsSection />)

    await waitFor(() => expect(mocks.getProjects).toHaveBeenCalledWith('team-1'))
    expect(await screen.findByText('Create your first project')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /new project/i }))
    expect(screen.getByText('Project form ready')).toBeInTheDocument()
  })

  it('shows who to ask when teams exist but project creation is unavailable', async () => {
    mocks.getTeams.mockResolvedValue([{ ...teamAlpha, canCreateProject: false }])
    mocks.getProjects.mockResolvedValue([])

    render(<ProjectsSection />)

    await waitFor(() => expect(mocks.getProjects).toHaveBeenCalledWith('team-1'))
    expect(
      await screen.findByText('Ask a team admin to let you create projects')
    ).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /new project/i })).not.toBeInTheDocument()
  })
})
