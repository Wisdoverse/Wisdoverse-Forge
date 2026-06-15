import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { TeamsSection } from '@app/pages/settings/ui/TeamsSection'
import { teamApi } from '@app/entities/team'

const authState = vi.hoisted(() => ({
  user: {
    id: 'user-1',
    role: 'owner',
    orgId: 'org-1',
  } as { id: string; role: string; orgId?: string },
}))

vi.mock('@app/shared/model/auth.context', () => ({
  useAuth: () => ({ user: authState.user }),
}))

vi.mock('@app/entities/user', () => ({
  userApi: {
    getUsers: vi.fn(),
  },
}))

vi.mock('@app/entities/team', () => ({
  teamApi: {
    getTeams: vi.fn(),
    createTeam: vi.fn(),
    updateTeam: vi.fn(),
    deleteTeam: vi.fn(),
    getMembers: vi.fn(),
    addMember: vi.fn(),
    updateMember: vi.fn(),
    removeMember: vi.fn(),
  },
}))

const getTeams = vi.mocked(teamApi.getTeams)
const createTeam = vi.mocked(teamApi.createTeam)

afterEach(() => {
  cleanup()
  authState.user = { id: 'user-1', role: 'owner', orgId: 'org-1' }
  vi.clearAllMocks()
})

describe('TeamsSection', () => {
  test('explains why teams matter before the first team exists', async () => {
    getTeams.mockResolvedValue([])

    render(<TeamsSection />)

    await waitFor(() => expect(getTeams).toHaveBeenCalledWith('org-1'))
    expect(screen.getByText('Teams and access')).toBeDefined()
    expect(
      screen.getByText(/teams keep people and projects together inside this team space/i)
    ).toBeDefined()
    expect(screen.queryByText(/access groups/i)).toBeNull()
    expect(screen.getByRole('button', { name: 'New Team' })).toBeDefined()
    expect(screen.getByText('Create a team first')).toBeDefined()
    expect(screen.getByText(/Teams keep projects and access together/i)).toBeDefined()
    expect(screen.queryByText(/teams group projects/i)).toBeNull()
    expect(screen.getByRole('button', { name: 'Create first team' })).toBeDefined()
  })

  test('guides users to choose a team space before creating teams', () => {
    authState.user = { id: 'user-1', role: 'owner' }

    render(<TeamsSection />)

    expect(getTeams).not.toHaveBeenCalled()
    expect(screen.getByText('Choose a team space first')).toBeDefined()
    expect(screen.getByText(/Select or create one before adding people/i)).toBeDefined()
    expect(screen.queryByText(/Choose an organization first/i)).toBeNull()
  })

  test('shows a beginner recovery step when teams cannot load', async () => {
    getTeams.mockRejectedValue(new Error('HTTP 500'))

    render(<TeamsSection />)

    await waitFor(() => expect(getTeams).toHaveBeenCalledWith('org-1'))
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Refresh Settings to load workspace teams. If it still fails, ask an owner or admin to check workspace setup.'
    )
    expect(screen.queryByText('HTTP 500')).toBeNull()
    expect(screen.queryByText(/temporarily unavailable/i)).toBeNull()
  })

  test('turns team loading permission errors into an owner access step', async () => {
    getTeams.mockRejectedValue(new Error('API 403: {"error":"owner role required"}'))

    render(<TeamsSection />)

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Ask an owner or admin to update your workspace access.')
    expect(alert.textContent).not.toContain('Detail:')
    expect(alert.textContent).not.toContain('owner role required')
  })

  test('turns team creation validation errors into field guidance', async () => {
    getTeams.mockResolvedValue([])
    createTeam.mockRejectedValue(new Error('API 422: {"message":"team name is required"}'))

    render(<TeamsSection />)

    await waitFor(() => expect(getTeams).toHaveBeenCalledWith('org-1'))
    fireEvent.click(screen.getByRole('button', { name: 'New Team' }))
    fireEvent.change(screen.getByLabelText(/team name/i), { target: { value: 'Design' } })
    fireEvent.click(screen.getByRole('button', { name: /create team/i }))

    expect(await screen.findByText(/Enter a team name, then try again/i)).toBeDefined()
    expect(screen.queryByText(/team name is required/i)).toBeNull()
  })

  test('opens team creation from the empty state action', async () => {
    getTeams.mockResolvedValue([])

    render(<TeamsSection />)

    await waitFor(() => expect(getTeams).toHaveBeenCalledWith('org-1'))
    fireEvent.click(screen.getByRole('button', { name: 'Create first team' }))

    expect(screen.getByText('Team setup path')).toBeDefined()
    expect(screen.getByLabelText(/team name/i)).toHaveFocus()
  })
})
