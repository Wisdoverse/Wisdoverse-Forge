import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import { OrganizationsPanel } from '@app/features/admin/OrganizationsPanel'
import { useAdminStore, type AdminOrg } from '@app/shared/model/admin.store'

const loadOrgsMock = vi.fn().mockResolvedValue(undefined)
const originalLoadOrgs = useAdminStore.getState().loadOrgs

const readyOrg: AdminOrg = {
  id: 'org-ready',
  name: 'Ready Org',
  slug: 'ready-org',
  plan: 'pro',
  membersCount: 4,
  teamsCount: 2,
  createdAt: '2026-01-01T00:00:00.000Z',
}

const missingTeamOrg: AdminOrg = {
  id: 'org-needs-team',
  name: 'Needs Team Org',
  slug: 'needs-team-org',
  plan: 'free',
  membersCount: 2,
  teamsCount: 0,
  createdAt: '2026-01-02T00:00:00.000Z',
}

const missingMemberOrg: AdminOrg = {
  id: 'org-needs-members',
  name: 'Needs Members Org',
  slug: 'needs-members-org',
  plan: 'enterprise',
  membersCount: 0,
  teamsCount: 1,
  createdAt: '2026-01-03T00:00:00.000Z',
}

beforeEach(() => {
  loadOrgsMock.mockClear()
  useAdminStore.setState({
    orgs: [readyOrg, missingTeamOrg, missingMemberOrg],
    orgsLoading: false,
    orgsError: null,
    loadOrgs: loadOrgsMock,
  })
})

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
  useAdminStore.setState({
    orgs: [],
    orgsLoading: false,
    orgsError: null,
    loadOrgs: originalLoadOrgs,
  })
})

describe('OrganizationsPanel', () => {
  test('summarizes organization setup and explains row readiness', async () => {
    render(<OrganizationsPanel />)

    await waitFor(() => expect(loadOrgsMock).toHaveBeenCalledWith())
    expect(screen.getByText('2 organizations need setup before teams can use them.')).toBeDefined()
    expect(screen.getByText('Showing 3 organizations with 6 members and 3 teams.')).toBeDefined()
    expect(screen.getByText('Organization URL name: ready-org')).toBeDefined()
    expect(screen.getByText('Ready for regular team work.')).toBeDefined()
    expect(screen.getByText('Ready to use')).toBeDefined()
    expect(
      screen.getByText('Members can create projects and start work from their teams.')
    ).toBeDefined()
    expect(screen.getByText('Needs a team')).toBeDefined()
    expect(
      screen.getByText('Create a team so members have a place to organize projects.')
    ).toBeDefined()
    expect(screen.getByText('Needs members')).toBeDefined()
    expect(
      screen.getByText('Invite at least one member so someone can use this organization.')
    ).toBeDefined()
  })

  test('shows a beginner-friendly empty state', () => {
    useAdminStore.setState({ orgs: [] })

    render(<OrganizationsPanel />)

    expect(screen.getByText('No organizations found.')).toBeDefined()
    expect(
      screen.getByText('Create or join an organization first, then it will appear here.')
    ).toBeDefined()
  })

  test('explains load errors with the next action', () => {
    useAdminStore.setState({
      orgs: [],
      orgsError: 'HTTP 403',
      orgsLoading: false,
    })

    render(<OrganizationsPanel />)

    expect(screen.getByRole('alert')).toHaveTextContent(
      'Organizations could not be loaded. Check your admin access and try again. Detail: HTTP 403'
    )
  })
})
