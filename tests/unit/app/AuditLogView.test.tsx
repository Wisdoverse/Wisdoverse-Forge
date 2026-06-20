import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { AuditLogView } from '@app/features/governance/AuditLogView'
import { orchestrationApi } from '@app/shared/api/orchestration'

vi.mock('@app/shared/api/orchestration', async () => {
  const actual = await vi.importActual<typeof import('@app/shared/api/orchestration')>(
    '@app/shared/api/orchestration'
  )
  return {
    ...actual,
    orchestrationApi: {
      ...actual.orchestrationApi,
      fetchGovernanceAudit: vi.fn(),
      exportGovernanceAudit: vi.fn(),
    },
  }
})

const fetchGovernanceAudit = vi.mocked(orchestrationApi.fetchGovernanceAudit)
const exportGovernanceAudit = vi.mocked(orchestrationApi.exportGovernanceAudit)

const auditResponse = {
  entries: [
    {
      id: 'audit-1',
      eventType: 'governance.context.feedback.recorded',
      actorUserId: 'user-1',
      itemKind: 'memory',
      scopeKind: 'project',
      scopeId: 'project-1',
      rawItemId: '11111111-1111-4111-8111-111111111111',
      auditSubjectHash: 'hash-visible',
      resourceType: 'memory_item',
      resourceId: '11111111-1111-4111-8111-111111111111',
      details: { label: 'useful' },
      detailsRedacted: false,
      tamperStatus: 'not_configured',
      createdAt: '2026-05-05T08:00:00.000Z',
    },
    {
      id: 'audit-2',
      eventType: 'governance.context.skill.approved',
      actorUserId: 'user-2',
      itemKind: 'skill',
      scopeKind: 'project',
      scopeId: 'project-hidden',
      rawItemId: null,
      auditSubjectHash: 'f9f0b5b53a25ad219cb741e8d15b3f2bb9a50f840b4f3300b814a7a2d18d2a66',
      resourceType: 'skill',
      resourceId: '22222222-2222-4222-8222-222222222222',
      details: { api_key: '[REDACTED]' },
      detailsRedacted: true,
      tamperStatus: 'valid',
      createdAt: '2026-05-05T09:00:00.000Z',
    },
  ],
  query: {
    eventPrefix: 'governance.context.',
    limit: 50,
    offset: 0,
    redacted: true,
  },
} as const

beforeEach(() => {
  fetchGovernanceAudit.mockResolvedValue(auditResponse)
  exportGovernanceAudit.mockResolvedValue(auditResponse)
})

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
})

describe('AuditLogView', () => {
  test('starts from common change views for first-time users', async () => {
    render(<AuditLogView />)

    await waitFor(() => expect(fetchGovernanceAudit).toHaveBeenCalledTimes(1))
    expect(screen.getByText('Start with what you need to check')).toBeDefined()
    expect(screen.getByText('See every saved note and saved instruction change.')).toBeDefined()
    expect(screen.getByText('Protected saved items')).toBeDefined()
    expect(screen.queryByText('Hidden item codes')).toBeNull()
    expect(screen.queryByText('Hidden item IDs')).toBeNull()
    expect(screen.getByText('Selected view')).toBeDefined()
    expect(screen.getAllByText('All saved item changes').length).toBeGreaterThan(0)

    const quickViews = screen.getByRole('group', { name: /common change views/i })
    expect(screen.queryByRole('group', { name: /common audit views/i })).toBeNull()
    fireEvent.click(
      within(quickViews).getByRole('button', { name: /saved instruction decisions/i })
    )

    await waitFor(() => expect(fetchGovernanceAudit).toHaveBeenCalledTimes(2))
    expect(
      within(quickViews).getByRole('button', { name: /saved instruction decisions/i })
    ).toHaveAttribute('aria-pressed', 'true')
    expect(within(quickViews).queryByRole('button', { name: /skill decisions/i })).toBeNull()
    expect(fetchGovernanceAudit).toHaveBeenLastCalledWith(
      expect.objectContaining({
        eventPrefix: 'governance.context.skill.',
        itemKind: 'skill',
        redactSecrets: true,
      })
    )
  })

  test('shows review codes without database wording and sends filters', async () => {
    render(<AuditLogView />)

    await waitFor(() => expect(fetchGovernanceAudit).toHaveBeenCalledTimes(1))
    expect(screen.getByText(/Hide secrets before export/i)).toBeDefined()
    expect(screen.getByText('Rows to show')).toBeDefined()
    expect(screen.queryByText('Record limit')).toBeNull()
    expect(screen.getByText('Changes shown')).toBeDefined()
    expect(screen.queryByText('History rows')).toBeNull()
    expect(screen.getByText('Hidden change-note rows')).toBeDefined()
    expect(screen.queryByText('Hidden review-note rows')).toBeNull()
    expect(screen.queryByText('Hidden detail rows')).toBeNull()
    expect(screen.queryByText('Hidden support-note rows')).toBeNull()
    expect(screen.getByLabelText('Refresh change history')).toBeDefined()
    expect(screen.queryByLabelText('Refresh audit history')).toBeNull()
    expect(screen.getByLabelText('Export change history')).toBeDefined()
    expect(screen.queryByLabelText('Export audit history')).toBeNull()
    expect(screen.getByText('Change area')).toBeDefined()
    expect(
      screen.getByText(/Paste a change area only when an owner or admin gives you one/i)
    ).toBeDefined()
    expect(screen.getByPlaceholderText(/Paste a change area only when needed/i)).toBeDefined()
    expect(screen.queryByText(/exact change area/i)).toBeNull()
    expect(screen.queryByText('Change category')).toBeNull()
    expect(screen.queryByText(/event category/i)).toBeNull()
    expect(screen.queryByText('Change group')).toBeNull()
    expect(screen.queryByText(/support event group/i)).toBeNull()
    expect(screen.getByText('Specific change name')).toBeDefined()
    expect(
      screen.getByText(
        'Optional. Use this only when an owner or admin gives you the exact change name.'
      )
    ).toBeDefined()
    expect(screen.queryByText('Exact event name')).toBeNull()
    expect(screen.queryByText(/exact event name/i)).toBeNull()
    expect(screen.queryByText('Support event name')).toBeNull()
    expect(screen.getByText('Exact work area')).toBeDefined()
    expect(
      screen.getByPlaceholderText(
        /team space, work area, team, or project reference only when an owner or admin gives you one/i
      )
    ).toBeDefined()
    expect(
      screen.queryByPlaceholderText(/exact team space, work area, team, or project/i)
    ).toBeNull()
    expect(screen.getByRole('option', { name: 'Team space' })).toBeDefined()
    expect(screen.getByRole('option', { name: 'Work area' })).toBeDefined()
    expect(screen.queryByRole('option', { name: 'Project area' })).toBeNull()
    expect(screen.queryByRole('option', { name: 'Project workspace' })).toBeNull()
    expect(screen.queryByRole('option', { name: 'Workspace' })).toBeNull()
    expect(screen.queryByRole('option', { name: 'Organization' })).toBeNull()
    expect(screen.getByText('Exact person')).toBeDefined()
    expect(
      screen.getByPlaceholderText(/Paste a person reference only when an owner or admin gives you one/i)
    ).toBeDefined()
    expect(screen.queryByPlaceholderText(/exact person (?:code|ID) only when needed/i)).toBeNull()
    expect(screen.queryByText(/work area support reference/i)).toBeNull()
    expect(screen.queryByText(/person support reference/i)).toBeNull()
    expect(screen.queryByPlaceholderText(/user ID when support asks for one/i)).toBeNull()
    expect(screen.getAllByTestId('governance-audit-row')).toHaveLength(2)
    expect(screen.getByText('Change')).toBeDefined()
    expect(screen.getByText('Feedback saved')).toBeDefined()
    expect(screen.queryByText('Feedback recorded')).toBeNull()
    expect(screen.getByText('Saved instruction saved for reuse')).toBeDefined()
    expect(screen.queryByText('Saved instruction approved for reuse')).toBeNull()
    expect(screen.queryByText('Saved instruction saved')).toBeNull()
    expect(screen.queryByText('Skill approved')).toBeNull()
    expect(screen.getAllByText('Show saved change name').length).toBeGreaterThan(0)
    expect(screen.queryByText('Show change details')).toBeNull()
    expect(screen.queryByText('Show event details')).toBeNull()
    expect(screen.queryByText('Show support event')).toBeNull()
    expect(screen.getByText('Saved note · Saved note details')).toBeDefined()
    expect(
      screen.queryByText(
        new RegExp(['Saved note', ['Memory', 'item'].join('\\s+')].join('.*'), 'i')
      )
    ).toBeNull()
    expect(screen.getByText('Saved instruction · Instruction details')).toBeDefined()
    expect(screen.queryByText('Saved instruction · Skill')).toBeNull()
    expect(screen.getAllByText('Changed item').length).toBeGreaterThan(0)
    expect(screen.getByText('Changed by')).toBeDefined()
    expect(screen.getByText('Person reference user-1')).toBeDefined()
    expect(screen.getByText('Person reference user-2')).toBeDefined()
    expect(screen.queryByText('Person code user-1')).toBeNull()
    expect(screen.queryByText('Person ID user-1')).toBeNull()
    expect(screen.queryByText('user-1')).toBeNull()
    expect(screen.getByText('Verification')).toBeDefined()
    expect(screen.getByText('Change notes')).toBeDefined()
    expect(screen.queryByText('Review notes')).toBeNull()
    expect(screen.getAllByText('Show change notes').length).toBeGreaterThan(0)
    expect(screen.queryByText('Show review notes')).toBeNull()
    expect(screen.queryByText('Support notes')).toBeNull()
    expect(screen.queryByText('Show support notes')).toBeNull()
    expect(screen.getByTestId('governance-audit-item-reference').textContent).toContain(
      'Visible saved item'
    )
    expect(screen.getByTestId('governance-audit-item-reference').textContent).not.toContain(
      'Visible item code'
    )
    expect(screen.getByTestId('governance-audit-item-reference').textContent).not.toContain(
      'Visible item ID'
    )
    expect(screen.getByTestId('governance-audit-item-reference').textContent).toContain('11111111')
    expect(screen.getByTestId('governance-audit-protected-reference').textContent).toContain(
      'Protected saved item'
    )
    expect(screen.getByTestId('governance-audit-protected-reference').textContent).not.toContain(
      'Hidden item code'
    )
    expect(screen.getByTestId('governance-audit-protected-reference').textContent).not.toContain(
      'Hidden item ID'
    )
    expect(screen.getByTestId('governance-audit-protected-reference').textContent).toContain(
      'f9f0b5b53a'
    )
    expect(screen.getAllByText('Project').length).toBeGreaterThan(0)
    expect(screen.getByText('Work area project-1')).toBeDefined()
    expect(screen.queryByText('Area code project-1')).toBeNull()
    expect(screen.queryByText('Area ID project-1')).toBeNull()
    expect(screen.queryByText(/Area support reference/i)).toBeNull()
    expect(screen.getByTestId('governance-audit-redacted').textContent).toContain(
      'Change notes hidden'
    )
    expect(screen.queryByText('Protected')).toBeNull()
    expect(screen.getByText('Set up verification')).toBeDefined()
    expect(screen.queryByText('Check proof setup')).toBeNull()
    expect(screen.getByText('Verified')).toBeDefined()

    fireEvent.change(screen.getByTestId('governance-audit-filter-event-type'), {
      target: { value: 'governance.context.skill.approved' },
    })
    fireEvent.change(screen.getByTestId('governance-audit-filter-item-kind'), {
      target: { value: 'skill' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Apply filters' }))

    await waitFor(() => expect(fetchGovernanceAudit).toHaveBeenCalledTimes(2))
    expect(fetchGovernanceAudit).toHaveBeenLastCalledWith(
      expect.objectContaining({
        eventType: 'governance.context.skill.approved',
        itemKind: 'skill',
        redactSecrets: true,
      })
    )
  })

  test('labels missing audit event and resource names with beginner checks', async () => {
    fetchGovernanceAudit.mockResolvedValueOnce({
      ...auditResponse,
      entries: [
        {
          ...auditResponse.entries[0],
          id: 'audit-missing-labels',
          eventType: '',
          itemKind: null,
          rawItemId: null,
          auditSubjectHash: 'missing-label-hash',
          resourceType: '',
          details: {},
        },
      ],
    })

    render(<AuditLogView />)

    expect(await screen.findByText('Check change')).toBeDefined()
    expect(screen.getByText('Item hidden for safety · Check item type')).toBeDefined()
    expect(screen.getByText('Show saved change name')).toBeDefined()
    expect(screen.getByText('Saved change name missing')).toBeDefined()
    expect(screen.queryByText('Show change details')).toBeNull()
    expect(screen.queryByText('Check change details')).toBeNull()
    expect(screen.queryByText('Check audit change')).toBeNull()
    expect(screen.queryByText('Show event details')).toBeNull()
    expect(screen.queryByText('Check event details')).toBeNull()
    expect(screen.queryByText('Show support event')).toBeNull()
    expect(screen.queryByText('Check support event')).toBeNull()
    expect(screen.queryByText('Change not listed')).toBeNull()
    expect(screen.queryByText('Resource not listed')).toBeNull()
    expect(screen.queryByText('not listed')).toBeNull()
    expect(screen.queryByText('Unknown')).toBeNull()
  })

  test('names failed verification directly instead of using proof jargon', async () => {
    fetchGovernanceAudit.mockResolvedValueOnce({
      ...auditResponse,
      entries: [
        {
          ...auditResponse.entries[0],
          id: 'audit-verification-invalid',
          tamperStatus: 'invalid',
        },
      ],
    })

    render(<AuditLogView />)

    expect(await screen.findByText('Check verification')).toBeDefined()
    expect(screen.queryByText('Review proof')).toBeNull()
    expect(screen.queryByText('Needs review')).toBeNull()
  })

  test('hides sensitive values in audit change details', async () => {
    fetchGovernanceAudit.mockResolvedValueOnce({
      ...auditResponse,
      entries: [
        {
          ...auditResponse.entries[0],
          details: {
            label: 'useful',
            token: 'audit-secret-token',
            nested: {
              apiKey: 'private-audit-key',
              error: 'Missing token',
            },
          },
        },
      ],
    })

    render(<AuditLogView />)

    await waitFor(() => expect(fetchGovernanceAudit).toHaveBeenCalledTimes(1))
    expect(screen.getAllByText(/Hidden for safety/i).length).toBeGreaterThan(0)
    expect(screen.getByText(/Reconnect the needed account access/i)).toBeDefined()
    expect(screen.queryByText(/Required account access is missing/i)).toBeNull()
    expect(screen.queryByText(/audit-secret-token/i)).toBeNull()
    expect(screen.queryByText(/private-audit-key/i)).toBeNull()
    expect(screen.queryByText(/Missing token/i)).toBeNull()
  })

  test('explains how to recover from an empty audit result', async () => {
    fetchGovernanceAudit.mockResolvedValueOnce({
      entries: [],
      query: {
        eventPrefix: 'governance.context.',
        limit: 50,
        offset: 0,
        redacted: true,
      },
    })

    render(<AuditLogView />)

    expect(await screen.findByText('Your filters may be hiding changes')).toBeDefined()
    expect(screen.queryByText('Your filters may be hiding audit history')).toBeNull()
    expect(screen.getByText(/Show all history first/i)).toBeDefined()
    expect(
      screen.getByText(/save a useful instruction or mark a saved note as helpful/i)
    ).toBeDefined()
    expect(screen.getByText(/then choose Show all change history/i)).toBeDefined()
    expect(screen.getByText(/new team space/i)).toBeDefined()
    expect(screen.queryByText(/new workspace/i)).toBeNull()
    expect(screen.queryByText(/approve a skill/i)).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: 'Show all change history' }))
    expect(screen.queryByRole('button', { name: 'Show all audit history' })).toBeNull()

    await waitFor(() => expect(fetchGovernanceAudit).toHaveBeenCalledTimes(2))
    expect(fetchGovernanceAudit).toHaveBeenLastCalledWith(
      expect.objectContaining({
        eventPrefix: 'governance.context.',
        itemKind: undefined,
        scopeKind: undefined,
        offset: 0,
        redactSecrets: true,
      })
    )
  })

  test('shows beginner network guidance when audit records cannot load', async () => {
    fetchGovernanceAudit.mockRejectedValueOnce(new TypeError('Failed to fetch'))

    render(<AuditLogView />)

    const error = await screen.findByRole('alert')
    expect(error).toHaveAttribute('aria-live', 'polite')
    expect(error.textContent).toContain(
      'Choose Refresh change history, then apply the filters again.'
    )
    expect(error.textContent).not.toContain('audit view')
    expect(error.textContent).toContain(
      'check your connection and choose Refresh change history again'
    )
    expect(error.textContent).not.toContain('refresh the page')
    expect(error.textContent).not.toMatch(/failed to fetch/i)
    expect(error.textContent).not.toContain('service')
  })

  test('shows beginner permission guidance when audit export fails', async () => {
    exportGovernanceAudit.mockRejectedValueOnce(new Error('403 Forbidden'))

    render(<AuditLogView />)

    await waitFor(() => expect(fetchGovernanceAudit).toHaveBeenCalledTimes(1))
    fireEvent.click(screen.getByTestId('governance-audit-export'))

    const error = await screen.findByRole('alert')
    expect(error).toHaveAttribute('aria-live', 'polite')
    expect(error.textContent).toContain('do not have permission')
    expect(error.textContent).toContain('owner or admin')
    expect(error.textContent).not.toContain('403 Forbidden')
  })
})
