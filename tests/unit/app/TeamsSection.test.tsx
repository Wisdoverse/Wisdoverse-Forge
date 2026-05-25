import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen, waitFor } from '@testing-library/react'
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
    expect(screen.getByRole('button', { name: 'Create team' })).toBeDefined()
    expect(screen.getByText('Create your first team')).toBeDefined()
    expect(screen.getByText(/Teams group people, projects, and access rules/i)).toBeDefined()
  })

  test('guides users to choose an organization before creating teams', () => {
    authState.user = { id: 'user-1', role: 'owner' }

    render(<TeamsSection />)

    expect(getTeams).not.toHaveBeenCalled()
    expect(screen.getByText('Choose an organization first')).toBeDefined()
    expect(screen.getByText(/Select or create one before adding people/i)).toBeDefined()
  })
})
