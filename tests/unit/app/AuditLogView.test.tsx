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
  test('starts from common audit views for first-time users', async () => {
    render(<AuditLogView />)

    await waitFor(() => expect(fetchGovernanceAudit).toHaveBeenCalledTimes(1))
    expect(screen.getByText('Start with what you need to check')).toBeDefined()
    expect(screen.getByText('See every saved-memory and saved-instruction change.')).toBeDefined()
    expect(screen.getByText('Hidden item references')).toBeDefined()
    expect(screen.getByText('Selected view')).toBeDefined()
    expect(screen.getAllByText('All context changes').length).toBeGreaterThan(0)

    const quickViews = screen.getByRole('group', { name: /common audit views/i })
    fireEvent.click(within(quickViews).getByRole('button', { name: /skill decisions/i }))

    await waitFor(() => expect(fetchGovernanceAudit).toHaveBeenCalledTimes(2))
    expect(within(quickViews).getByRole('button', { name: /skill decisions/i })).toHaveAttribute(
      'aria-pressed',
      'true'
    )
    expect(fetchGovernanceAudit).toHaveBeenLastCalledWith(
      expect.objectContaining({
        eventPrefix: 'governance.context.skill.',
        itemKind: 'skill',
        redactSecrets: true,
      })
    )
  })

  test('shows support references without database wording and sends filters', async () => {
    render(<AuditLogView />)

    await waitFor(() => expect(fetchGovernanceAudit).toHaveBeenCalledTimes(1))
    expect(screen.getByText(/Hide secrets before export/i)).toBeDefined()
    expect(screen.getByText('Change group')).toBeDefined()
    expect(
      screen.getByText('Use the default unless support gives you a specific group.')
    ).toBeDefined()
    expect(screen.getByText('Support event name')).toBeDefined()
    expect(
      screen.getByText('Optional. Paste this only when support asks for a specific event.')
    ).toBeDefined()
    expect(screen.getByText('Work area reference')).toBeDefined()
    expect(
      screen.getByPlaceholderText(/organization, workspace, team, or project reference/i)
    ).toBeDefined()
    expect(screen.getByText('Person reference')).toBeDefined()
    expect(screen.getByPlaceholderText(/user reference when needed/i)).toBeDefined()
    expect(screen.getAllByTestId('governance-audit-row')).toHaveLength(2)
    expect(screen.getByText('Change')).toBeDefined()
    expect(screen.getByText('Feedback recorded')).toBeDefined()
    expect(screen.getByText('Skill approved')).toBeDefined()
    expect(screen.getAllByText('Show support event').length).toBeGreaterThan(0)
    expect(screen.getByText('Saved memory · Memory item')).toBeDefined()
    expect(screen.getByText('Saved instruction · Skill')).toBeDefined()
    expect(screen.getAllByText('Changed item').length).toBeGreaterThan(0)
    expect(screen.getByText('Changed by')).toBeDefined()
    expect(screen.getByText('Verification')).toBeDefined()
    expect(screen.getByText('Support notes')).toBeDefined()
    expect(screen.getAllByText('Show support notes').length).toBeGreaterThan(0)
    expect(screen.getByTestId('governance-audit-item-reference').textContent).toContain(
      'Visible item reference'
    )
    expect(screen.getByTestId('governance-audit-item-reference').textContent).toContain('11111111')
    expect(screen.getByTestId('governance-audit-protected-reference').textContent).toContain(
      'Hidden item reference'
    )
    expect(screen.getByTestId('governance-audit-protected-reference').textContent).toContain(
      'f9f0b5b53a'
    )
    expect(screen.getAllByText('Project').length).toBeGreaterThan(0)
    expect(screen.getByText('Reference project-1')).toBeDefined()
    expect(screen.queryByText(/Work area I[D]/)).toBeNull()
    expect(screen.queryByText(/Person I[D]/)).toBeNull()
    expect(screen.getByTestId('governance-audit-redacted').textContent).toContain('Protected')
    expect(screen.getByText('Not checked')).toBeDefined()
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

  test('labels missing audit event and resource names without Unknown', async () => {
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

    expect(await screen.findByText('Change not listed')).toBeDefined()
    expect(screen.getByText('Item hidden for safety · Resource not listed')).toBeDefined()
    expect(screen.getByText('Show support event')).toBeDefined()
    expect(screen.getByText('not listed')).toBeDefined()
    expect(screen.queryByText('Unknown')).toBeNull()
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
    expect(screen.getByText(/Required account access is missing/i)).toBeDefined()
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

    expect(await screen.findByText('No audit history in this view')).toBeDefined()
    expect(screen.getByText(/Try All context changes or widen the time range/i)).toBeDefined()
    expect(screen.getByText(/approve a skill or record context feedback/i)).toBeDefined()
  })

  test('shows beginner network guidance when audit records cannot load', async () => {
    fetchGovernanceAudit.mockRejectedValueOnce(new TypeError('Failed to fetch'))

    render(<AuditLogView />)

    const error = await screen.findByRole('alert')
    expect(error.textContent).toContain('Governance audit history could not load')
    expect(error.textContent).toContain('Forge could not connect while loading audit history')
    expect(error.textContent).not.toMatch(/failed to fetch/i)
    expect(error.textContent).not.toContain('service')
  })

  test('shows beginner permission guidance when audit export fails', async () => {
    exportGovernanceAudit.mockRejectedValueOnce(new Error('403 Forbidden'))

    render(<AuditLogView />)

    await waitFor(() => expect(fetchGovernanceAudit).toHaveBeenCalledTimes(1))
    fireEvent.click(screen.getByTestId('governance-audit-export'))

    const error = await screen.findByRole('alert')
    expect(error.textContent).toContain('do not have permission')
    expect(error.textContent).toContain('owner or admin')
    expect(error.textContent).not.toContain('403 Forbidden')
  })
})
