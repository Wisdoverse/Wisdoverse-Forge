import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
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

beforeEach(() => {
  fetchGovernanceAudit.mockResolvedValue({
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
  })
})

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
})

describe('AuditLogView', () => {
  test('renders raw IDs only for visible subjects and sends filters', async () => {
    render(<AuditLogView />)

    await waitFor(() => expect(fetchGovernanceAudit).toHaveBeenCalledTimes(1))
    expect(screen.getByTestId('governance-audit-review-path')).toBeDefined()
    expect(screen.getByText('Audit review path')).toBeDefined()
    expect(screen.getByText(/keep redact secrets on/i)).toBeDefined()
    expect(screen.getAllByTestId('governance-audit-row')).toHaveLength(2)
    expect(screen.getByText('Records')).toBeDefined()
    expect(screen.getAllByText('Changed item').length).toBeGreaterThan(0)
    expect(screen.getByText('Changed by')).toBeDefined()
    expect(screen.getByText('Verification')).toBeDefined()
    expect(screen.getByTestId('governance-audit-raw-item-id').textContent).toContain('11111111')
    expect(screen.getByTestId('governance-audit-subject-hash').textContent).toContain('f9f0b5b53a')
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

    expect(await screen.findByText('No governance audit events')).toBeDefined()
    expect(screen.getByText(/clear narrow filters or increase the limit/i)).toBeDefined()
  })
})
