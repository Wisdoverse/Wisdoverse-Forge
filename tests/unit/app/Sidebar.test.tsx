import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { render, screen, fireEvent, cleanup, waitFor, within } from '@testing-library/react'
import { Sidebar } from '@app/layouts/sidebar'
import { projectApi } from '@app/entities/project'
import { teamApi } from '@app/entities/team'
import { useNavigationStore } from '@app/entities/navigation'

vi.mock('@app/entities/team', () => ({
  teamApi: {
    getTeams: vi.fn(),
    updateTeam: vi.fn().mockResolvedValue(undefined),
    deleteTeam: vi.fn().mockResolvedValue(undefined),
  },
}))

vi.mock('@app/entities/project', () => ({
  projectApi: {
    getProjects: vi.fn(),
    updateProject: vi.fn().mockResolvedValue(undefined),
    deleteProject: vi.fn().mockResolvedValue(undefined),
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

vi.mock('@app/shared/model/auth.context', () => ({
  useAuth: () => ({
    authManager: { logout: vi.fn() },
    user: { role: 'user' },
    isAuthenticated: true,
    isLoading: false,
  }),
}))

afterEach(cleanup)

beforeEach(() => {
  vi.clearAllMocks()
  useNavigationStore.getState().reset()
})

function seedProjectTree() {
  useNavigationStore.setState({
    orgs: [{ id: 'org1', name: 'Org', slug: 'org', plan: 'pro', role: 'owner' }],
    selectedOrgId: 'org1',
    sidebarExpanded: true,
    teams: [
      {
        id: 't1',
        orgId: 'org1',
        name: 'Team Alpha',
        slug: 'team-alpha',
        visibility: 'open',
        description: '',
      },
    ],
    projects: {
      t1: [
        {
          id: 'p1',
          teamId: 't1',
          name: 'Project X',
          slug: 'proj-x',
          color: '#007AFF',
          description: '',
        },
      ],
    },
    expandedTeams: ['t1'],
    selectedProjectId: null,
  })
}

describe('Sidebar', () => {
  it('renders expanded sidebar with org name and nav items', () => {
    useNavigationStore.setState({
      orgs: [{ id: 'org1', name: 'My Org', slug: 'my-org', plan: 'pro', role: 'owner' }],
      selectedOrgId: 'org1',
      sidebarExpanded: true,
      teams: [],
      projects: {},
    })

    const onNavigate = vi.fn()
    render(<Sidebar activePath="/tasks" onNavigate={onNavigate} />)

    expect(screen.getByTestId('sidebar')).toBeInTheDocument()
    expect(screen.getByText('Wisdoverse Forge')).toBeInTheDocument()
    expect(screen.getByText('My Org')).toBeInTheDocument()
    expect(screen.getByTestId('sidebar-nav-tasks')).toBeInTheDocument()
    expect(screen.getByTestId('sidebar-nav-agents')).toBeInTheDocument()
    expect(screen.getByTestId('project-tree-empty-teams')).toHaveTextContent('Create a team first')
    fireEvent.click(screen.getByRole('button', { name: /open team settings/i }))
    expect(onNavigate).toHaveBeenCalledWith('/settings/teams')
  })

  it('renders collapsed sidebar with only icons', () => {
    useNavigationStore.setState({
      orgs: [{ id: 'org1', name: 'My Org', slug: 'my-org', plan: 'pro', role: 'owner' }],
      selectedOrgId: 'org1',
      sidebarExpanded: false,
      teams: [],
      projects: {},
    })

    render(<Sidebar activePath="/tasks" onNavigate={vi.fn()} />)

    expect(screen.getByTestId('sidebar')).toBeInTheDocument()
    expect(screen.queryByText('My Org')).not.toBeInTheDocument()
    expect(screen.queryByText('Wisdoverse Forge')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Expand sidebar' })).toBeInTheDocument()
  })

  it('clicking nav item calls onNavigate', () => {
    useNavigationStore.setState({
      orgs: [{ id: 'org1', name: 'My Org', slug: 'my-org', plan: 'pro', role: 'owner' }],
      selectedOrgId: 'org1',
      sidebarExpanded: true,
      teams: [],
      projects: {},
    })

    const onNavigate = vi.fn()
    render(<Sidebar activePath="/tasks" onNavigate={onNavigate} />)

    fireEvent.click(screen.getByTestId('sidebar-nav-agents'))
    expect(onNavigate).toHaveBeenCalledWith('/agents')
  })

  it('renders project tree with teams and projects', () => {
    seedProjectTree()

    render(<Sidebar activePath="/tasks" onNavigate={vi.fn()} />)

    expect(screen.getByText('Team Alpha')).toBeInTheDocument()
    expect(screen.getByText('Project X')).toBeInTheDocument()
  })

  it('guides users to create a project when a team is empty', () => {
    useNavigationStore.setState({
      orgs: [{ id: 'org1', name: 'Org', slug: 'org', plan: 'pro', role: 'owner' }],
      selectedOrgId: 'org1',
      sidebarExpanded: true,
      teams: [
        {
          id: 't1',
          orgId: 'org1',
          name: 'Team Alpha',
          slug: 'team-alpha',
          visibility: 'open',
          description: '',
        },
      ],
      projects: { t1: [] },
      expandedTeams: ['t1'],
      selectedProjectId: null,
    })
    const onNavigate = vi.fn()

    render(<Sidebar activePath="/tasks" onNavigate={onNavigate} />)

    expect(screen.getByTestId('team-t1-empty-projects')).toHaveTextContent(
      "Add this team's first project"
    )
    fireEvent.click(screen.getByRole('button', { name: /open project settings/i }))
    expect(onNavigate).toHaveBeenCalledWith('/settings/projects')
  })

  it('opens project context menu on right click', () => {
    seedProjectTree()
    const onCreateTaskForProject = vi.fn()

    render(
      <Sidebar
        activePath="/tasks"
        onNavigate={vi.fn()}
        onCreateTaskForProject={onCreateTaskForProject}
      />
    )
    fireEvent.contextMenu(screen.getByTestId('project-p1'))

    const menu = screen.getByTestId('project-context-menu')
    const menuScope = within(menu)

    expect(menu).toHaveAttribute('role', 'menu')
    expect(menu).toHaveAttribute('aria-label', 'Project X project menu')
    expect(menuScope.getByText('Team Alpha team · link name proj-x')).toBeInTheDocument()
    expect(menuScope.getByRole('menuitem', { name: /open project board/i })).toBeInTheDocument()
    expect(menuScope.getByRole('menuitem', { name: /create task here/i })).toBeInTheDocument()
    expect(menuScope.getByRole('menuitem', { name: /share project/i })).toBeInTheDocument()
    expect(menuScope.getByRole('menuitem', { name: /rename project/i })).toBeInTheDocument()
    expect(menuScope.getByRole('menuitem', { name: /all project settings/i })).toBeInTheDocument()
    expect(menuScope.getByRole('menuitem', { name: /copy support id/i })).toBeInTheDocument()
    expect(menuScope.getByRole('menuitem', { name: /copy link name/i })).toBeInTheDocument()
    expect(menuScope.getByText(/use when an admin asks/i)).toBeInTheDocument()
    expect(menuScope.getByText(/appears in project links/i)).toBeInTheDocument()
    expect(menuScope.getByRole('menuitem', { name: /delete project/i })).toBeInTheDocument()
  })

  it('opens team context menu on right click', () => {
    seedProjectTree()

    render(<Sidebar activePath="/tasks" onNavigate={vi.fn()} />)
    fireEvent.contextMenu(screen.getByTestId('team-t1'))

    expect(screen.getByRole('menu', { name: /team alpha team menu/i })).toBeInTheDocument()
    expect(screen.getByRole('menuitem', { name: /configure team/i })).toBeInTheDocument()
    expect(screen.getByRole('menuitem', { name: /delete team/i })).toBeInTheDocument()
  })

  it('does not open team context menu without team management permission', () => {
    seedProjectTree()
    useNavigationStore.setState((state) => ({
      teams: state.teams.map((team) => ({ ...team, canManage: false, canDelete: false })),
    }))

    render(<Sidebar activePath="/tasks" onNavigate={vi.fn()} />)
    fireEvent.contextMenu(screen.getByTestId('team-t1'))

    expect(screen.queryByRole('menu', { name: /team alpha team menu/i })).not.toBeInTheDocument()
  })

  it('opens a limited project context menu without project management permission', () => {
    seedProjectTree()
    useNavigationStore.setState((state) => ({
      projects: {
        t1: (state.projects.t1 ?? []).map((project) => ({
          ...project,
          canManage: false,
          canDelete: false,
        })),
      },
    }))

    render(<Sidebar activePath="/tasks" onNavigate={vi.fn()} />)
    fireEvent.contextMenu(screen.getByTestId('project-p1'))

    expect(screen.getByRole('menu', { name: /project x project menu/i })).toBeInTheDocument()
    expect(screen.getByRole('menuitem', { name: /open project board/i })).toBeInTheDocument()
    expect(screen.getByRole('menuitem', { name: /copy support id/i })).toBeInTheDocument()
    expect(screen.queryByRole('menuitem', { name: /share project/i })).not.toBeInTheDocument()
    expect(screen.queryByRole('menuitem', { name: /rename project/i })).not.toBeInTheDocument()
    expect(screen.queryByRole('menuitem', { name: /delete project/i })).not.toBeInTheDocument()
  })

  it('opens project from the context menu', async () => {
    seedProjectTree()
    const onNavigate = vi.fn()

    render(<Sidebar activePath="/agents" onNavigate={onNavigate} />)
    fireEvent.contextMenu(screen.getByTestId('project-p1'))
    fireEvent.click(screen.getByRole('menuitem', { name: /open project board/i }))

    await waitFor(() => expect(useNavigationStore.getState().selectedProjectId).toBe('p1'))
    expect(onNavigate).toHaveBeenCalledWith('/tasks')
  })

  it('starts task creation from the project context menu', async () => {
    seedProjectTree()
    const onCreateTaskForProject = vi.fn()

    render(
      <Sidebar
        activePath="/tasks"
        onNavigate={vi.fn()}
        onCreateTaskForProject={onCreateTaskForProject}
      />
    )
    fireEvent.contextMenu(screen.getByTestId('project-p1'))
    fireEvent.click(screen.getByRole('menuitem', { name: /create task here/i }))

    await waitFor(() => expect(onCreateTaskForProject).toHaveBeenCalledWith('p1'))
  })

  it('copies project menu values with visible beginner feedback', async () => {
    seedProjectTree()
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    })

    render(<Sidebar activePath="/tasks" onNavigate={vi.fn()} />)
    fireEvent.contextMenu(screen.getByTestId('project-p1'))
    fireEvent.click(screen.getByRole('menuitem', { name: /copy support id/i }))

    await waitFor(() => expect(writeText).toHaveBeenCalledWith('p1'))
    expect(screen.getByTestId('project-copy-status')).toHaveTextContent('Project support ID copied')
  })

  it('configures team name from context menu', async () => {
    seedProjectTree()
    vi.mocked(teamApi.updateTeam).mockResolvedValue({
      id: 't1',
      orgId: 'org1',
      name: 'Renamed Team',
      slug: 'team-alpha',
      visibility: 'open',
      description: '',
    })

    render(<Sidebar activePath="/tasks" onNavigate={vi.fn()} />)
    fireEvent.contextMenu(screen.getByTestId('team-t1'))
    fireEvent.click(screen.getByRole('menuitem', { name: /configure team/i }))
    fireEvent.change(screen.getByLabelText(/team name/i), {
      target: { value: 'Renamed Team' },
    })
    fireEvent.click(screen.getByRole('button', { name: /save/i }))

    await waitFor(() =>
      expect(teamApi.updateTeam).toHaveBeenCalledWith('org1', 't1', {
        name: 'Renamed Team',
      })
    )
    expect(screen.getByText('Renamed Team')).toBeInTheDocument()
  })

  it('deletes team from context menu', async () => {
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true)
    seedProjectTree()

    render(<Sidebar activePath="/tasks" onNavigate={vi.fn()} />)
    fireEvent.contextMenu(screen.getByTestId('team-t1'))
    fireEvent.click(screen.getByRole('menuitem', { name: /delete team/i }))

    await waitFor(() => expect(teamApi.deleteTeam).toHaveBeenCalledWith('org1', 't1'))
    expect(screen.queryByText('Team Alpha')).not.toBeInTheDocument()
    expect(screen.queryByText('Project X')).not.toBeInTheDocument()
    confirmSpy.mockRestore()
  })

  it('configures project name from context menu', async () => {
    seedProjectTree()
    vi.mocked(projectApi.updateProject).mockResolvedValue({
      id: 'p1',
      teamId: 't1',
      name: 'Renamed Project',
      slug: 'proj-x',
      color: '#007AFF',
      description: '',
    })

    render(<Sidebar activePath="/tasks" onNavigate={vi.fn()} />)
    fireEvent.contextMenu(screen.getByTestId('project-p1'))
    fireEvent.click(screen.getByRole('menuitem', { name: /rename project/i }))
    fireEvent.change(screen.getByLabelText(/project name/i), {
      target: { value: 'Renamed Project' },
    })
    fireEvent.click(screen.getByRole('button', { name: /save/i }))

    await waitFor(() =>
      expect(projectApi.updateProject).toHaveBeenCalledWith('t1', 'p1', {
        name: 'Renamed Project',
      })
    )
    expect(screen.getByText('Renamed Project')).toBeInTheDocument()
  })

  it('deletes project from context menu', async () => {
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true)
    seedProjectTree()

    render(<Sidebar activePath="/tasks" onNavigate={vi.fn()} />)
    fireEvent.contextMenu(screen.getByTestId('project-p1'))
    fireEvent.click(screen.getByRole('menuitem', { name: /delete project/i }))

    await waitFor(() => expect(projectApi.deleteProject).toHaveBeenCalledWith('t1', 'p1'))
    expect(screen.queryByText('Project X')).not.toBeInTheDocument()
    confirmSpy.mockRestore()
  })
})
