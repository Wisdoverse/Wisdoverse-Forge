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
    expect(screen.getByText('Teams and access groups')).toBeDefined()
    expect(screen.getByText(/teams group people and projects/i)).toBeDefined()
    expect(screen.getByRole('button', { name: 'New Team' })).toBeDefined()
    expect(screen.getByText('Create a team first')).toBeDefined()
    expect(screen.getByText(/Teams group projects and decide who can manage work/i)).toBeDefined()
  })

  test('guides users to choose an organization before creating teams', () => {
    authState.user = { id: 'user-1', role: 'owner' }

    render(<TeamsSection />)

    expect(getTeams).not.toHaveBeenCalled()
    expect(screen.getByText('Choose an organization first')).toBeDefined()
    expect(screen.getByText(/Select or create one before adding people/i)).toBeDefined()
  })

  test('turns team loading permission errors into an owner access step', async () => {
    getTeams.mockRejectedValue(new Error('API 403: {"error":"owner role required"}'))

    render(<TeamsSection />)

    expect(await screen.findByText(/You do not have permission to load teams/i)).toBeDefined()
    expect(screen.getByText(/Ask an owner or admin to update your access/i)).toBeDefined()
  })

  test('turns team creation validation errors into field guidance', async () => {
    getTeams.mockResolvedValue([])
    createTeam.mockRejectedValue(new Error('API 422: {"message":"team name is required"}'))

    render(<TeamsSection />)

    await waitFor(() => expect(getTeams).toHaveBeenCalledWith('org-1'))
    fireEvent.click(screen.getByRole('button', { name: 'New Team' }))
    fireEvent.change(screen.getByLabelText(/team name/i), { target: { value: 'Design' } })
    fireEvent.click(screen.getByRole('button', { name: 'Create Team' }))

    expect(await screen.findByText(/Check the team name, then try again/i)).toBeDefined()
    expect(screen.getByText(/team name is required/i)).toBeDefined()
  })
})
