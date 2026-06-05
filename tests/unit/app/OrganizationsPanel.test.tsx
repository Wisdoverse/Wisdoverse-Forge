import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen, within } from '@testing-library/react'
import { OrganizationsPanel } from '@app/features/admin/OrganizationsPanel'
import type { AdminOrg } from '@app/shared/model/admin.store'
import { useAdminStore } from '@app/shared/model/admin.store'

const loadOrgsMock = vi.fn().mockResolvedValue(undefined)
const originalLoadOrgs = useAdminStore.getState().loadOrgs

const organizations: AdminOrg[] = [
  {
    id: 'org-1',
    name: 'Acme Labs',
    slug: 'acme',
    plan: 'enterprise',
    membersCount: 6,
    teamsCount: 2,
    createdAt: '2026-05-01T10:00:00.000Z',
  },
  {
    id: 'org-2',
    name: 'Beta Team',
    slug: 'beta',
    plan: 'free',
    membersCount: 2,
    teamsCount: 1,
    createdAt: '2026-05-02T10:00:00.000Z',
  },
]

beforeEach(() => {
  loadOrgsMock.mockClear()
  useAdminStore.setState({
    orgs: organizations,
    orgsLoading: false,
    orgsError: null,
    loadOrgs: loadOrgsMock,
  })
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  useAdminStore.setState({
    orgs: [],
    orgsLoading: false,
    orgsError: null,
    loadOrgs: originalLoadOrgs,
  })
})

describe('OrganizationsPanel', () => {
  test('explains organization health signals before the admin table', async () => {
    render(<OrganizationsPanel />)

    const guide = await screen.findByTestId('admin-org-guide')
    expect(
      within(guide).getByText('Use organizations to check people and team setup at a glance')
    ).toBeDefined()
    expect(
      within(guide).getByText('8 members and 3 teams are spread across 2 organizations.')
    ).toBeDefined()
    expect(within(guide).getByText('Plan shows limits')).toBeDefined()
    expect(within(guide).getByText('Members show who has access')).toBeDefined()
    expect(within(guide).getByText('Teams show work areas')).toBeDefined()
    expect(within(guide).queryByText(/routing shape/i)).toBeNull()

    expect(screen.getByText('Acme Labs')).toBeDefined()
    expect(screen.getByText('Link name: acme')).toBeDefined()
    expect(screen.queryByText(/Organization URL name/i)).toBeNull()
    expect(screen.getByText('Enterprise')).toBeDefined()
    expect(screen.getAllByText('Review access when membership or teams change.').length).toBe(2)
    expect(loadOrgsMock).toHaveBeenCalled()
  })

  test('guides administrators when no organizations are visible', async () => {
    useAdminStore.setState({ orgs: [] })

    render(<OrganizationsPanel />)

    const guide = await screen.findByTestId('admin-org-guide')
    expect(
      within(guide).getByText(
        'Organizations appear here after setup. Teams, projects, and members need an organization first.'
      )
    ).toBeDefined()

    const emptyState = screen.getByTestId('admin-org-empty')
    expect(within(emptyState).getByText('No organizations are visible yet')).toBeDefined()
    expect(within(emptyState).getByText(/create one organization first/i)).toBeDefined()
    expect(within(emptyState).queryByText(/sync/i)).toBeNull()
    expect(within(emptyState).getByText(/one row with a plan, member count/i)).toBeDefined()
    expect(within(emptyState).getByText(/then create teams, projects, members/i)).toBeDefined()
  })

  test('adds recovery guidance when organizations fail to load', async () => {
    useAdminStore.setState({ orgsError: 'HTTP 503' })

    render(<OrganizationsPanel />)

    const error = await screen.findByTestId('admin-org-error')
    expect(within(error).getByText('The admin organizations could not load.')).toBeDefined()
    expect(within(error).queryByText('HTTP 503')).toBeNull()
    expect(
      within(error).getByText(
        'Refresh Admin, then try again. If it still fails, ask an owner or admin to check Admin setup and your role.'
      )
    ).toBeDefined()
    expect(within(error).queryByText(/admin service/i)).toBeNull()
  })
})
