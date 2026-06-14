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
    membersCount: 6,
    teamsCount: 2,
    createdAt: '2026-05-01T10:00:00.000Z',
  },
  {
    id: 'org-2',
    name: 'Beta Team',
    slug: 'beta',
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
    expect(within(guide).getByText('Use team spaces to check setup at a glance')).toBeDefined()
    expect(
      within(guide).getByText('8 people and 3 teams are spread across 2 team spaces.')
    ).toBeDefined()
    expect(within(guide).getByText('Setup status shows what is missing')).toBeDefined()
    expect(within(guide).queryByText('Readiness shows setup gaps')).toBeNull()
    expect(within(guide).getByText('People show access size')).toBeDefined()
    expect(within(guide).getByText('Teams show work areas')).toBeDefined()
    expect(within(guide).queryByText(/routing shape/i)).toBeNull()

    expect(screen.getByRole('heading', { name: 'Team spaces' })).toBeDefined()
    expect(screen.getByRole('columnheader', { name: 'Setup status' })).toBeDefined()
    expect(screen.queryByRole('columnheader', { name: 'Readiness' })).toBeNull()
    expect(screen.getByText('Acme Labs')).toBeDefined()
    // The backend has no plan data — the panel must not pretend it does.
    expect(screen.queryByText('Plan')).toBeNull()
    expect(screen.queryByText('Enterprise')).toBeNull()
    expect(screen.getByText('6')).toBeDefined()
    expect(screen.getAllByText('2').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Review access when people or teams change.').length).toBe(2)
    expect(loadOrgsMock).toHaveBeenCalled()
  })

  test('labels invalid created dates for team spaces without leaking Invalid Date', async () => {
    useAdminStore.setState({
      orgs: [{ ...organizations[0], createdAt: 'not-a-date' }],
    })

    render(<OrganizationsPanel />)

    expect(await screen.findByText('Refresh team spaces to check created date')).toBeDefined()
    expect(screen.queryByText('Invalid Date')).toBeNull()
    expect(screen.queryByText('—')).toBeNull()
  })

  test('guides administrators when no organizations are visible', async () => {
    useAdminStore.setState({ orgs: [] })

    render(<OrganizationsPanel />)

    const guide = await screen.findByTestId('admin-org-guide')
    expect(
      within(guide).getByText(
        'Team spaces appear here after setup or sync. Create one before adding teams, projects, people, or agent work queues.'
      )
    ).toBeDefined()

    const emptyState = screen.getByTestId('admin-org-empty')
    expect(within(emptyState).getByText('Create or sync a team space first')).toBeDefined()
    expect(
      within(emptyState).getByText(/Create or sync a team space before creating teams/i)
    ).toBeDefined()
    expect(within(emptyState).queryByText('No team spaces are visible yet')).toBeNull()
  })

  test('adds recovery guidance when organizations fail to load', async () => {
    useAdminStore.setState({ orgsError: 'HTTP 503' })

    render(<OrganizationsPanel />)

    const error = await screen.findByTestId('admin-org-error')
    expect(error).toHaveAttribute('aria-live', 'polite')
    expect(within(error).getByText('Refresh Admin to reload the team space list.')).toBeDefined()
    expect(
      within(error).getByText(
        'Refresh Admin, then try again. If it still fails, ask an owner or admin to check Admin setup and your role.'
      )
    ).toBeDefined()
    expect(within(error).queryByText('HTTP 503')).toBeNull()
  })
})
