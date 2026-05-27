import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, test } from 'vitest'
import { ContextEvidenceList } from '@app/features/detail/ContextEvidenceList'
import type { AppliedContextItem, TaskContextEvidence } from '@shared/types/context'

const createdAt = new Date('2026-05-06T07:00:00.000Z').toISOString()

function evidence(overrides: Partial<TaskContextEvidence> = {}): TaskContextEvidence {
  return {
    runId: overrides.runId ?? 'run-1',
    sourceType: overrides.sourceType ?? 'task_result',
    sourceId: overrides.sourceId ?? 'result-1',
    agentId: overrides.agentId ?? 'agent-1',
    payload: overrides.payload ?? {
      ok: true,
      summary: 'Health check completed successfully.',
    },
    createdAt: overrides.createdAt ?? createdAt,
  }
}

function revokedItem(overrides: Partial<AppliedContextItem> = {}): AppliedContextItem {
  return {
    injectionId: overrides.injectionId ?? 'injection-1',
    runId: overrides.runId ?? 'run-1',
    itemId: overrides.itemId ?? 'memory-1',
    itemKind: overrides.itemKind ?? 'memory',
    position: overrides.position ?? 0,
    title: overrides.title ?? 'Old deployment memory',
    contentPreview: overrides.contentPreview ?? 'Use the old deploy path.',
    contentTruncated: overrides.contentTruncated ?? false,
    contentRef: overrides.contentRef ?? 'memory_items/memory-1',
    scopeKind: overrides.scopeKind ?? 'project',
    scopeId: overrides.scopeId ?? 'project-1',
    sensitivity: overrides.sensitivity ?? 'internal',
    state: overrides.state ?? 'revoked',
    revoked: overrides.revoked ?? true,
    sourceTaskId: overrides.sourceTaskId ?? 'task-1',
    sourceRunId: overrides.sourceRunId ?? 'run-1',
    source: overrides.source ?? null,
    lastUsedAt: overrides.lastUsedAt ?? createdAt,
    lastVerifiedAt: overrides.lastVerifiedAt ?? createdAt,
    appliedAt: overrides.appliedAt ?? createdAt,
    adapter: overrides.adapter ?? 'claude',
    envelopeVersion: overrides.envelopeVersion ?? 'v1',
    capabilityProfile: overrides.capabilityProfile ?? {},
    degradationReason: overrides.degradationReason ?? null,
    feedback: overrides.feedback ?? null,
  }
}

afterEach(cleanup)

describe('ContextEvidenceList', () => {
  test('does not render when there is no evidence or revoked context', () => {
    const { container } = render(<ContextEvidenceList evidence={[]} revokedItems={[]} />)

    expect(container).toBeEmptyDOMElement()
  })

  test('explains task result evidence before showing technical details', () => {
    render(<ContextEvidenceList evidence={[evidence()]} revokedItems={[]} />)

    expect(screen.getByText('Evidence')).toBeInTheDocument()
    expect(screen.getByText(/what the agent used or produced/i)).toBeInTheDocument()
    expect(screen.getByText('Task result')).toBeInTheDocument()
    expect(
      screen.getByText(/Final output or status captured from the agent run/i)
    ).toBeInTheDocument()
    expect(screen.getByText('Health check completed successfully.')).toBeInTheDocument()
    expect(screen.getByText('Technical details')).toBeInTheDocument()
  })

  test('uses a plain-language fallback for unknown evidence sources', () => {
    render(
      <ContextEvidenceList
        evidence={[
          evidence({
            sourceType: 'custom_probe',
            payload: { ok: false, code: 'needs-review' },
          }),
        ]}
        revokedItems={[]}
      />
    )

    expect(screen.getByText('Custom Probe')).toBeInTheDocument()
    expect(screen.getByText('Supporting information recorded during the run.')).toBeInTheDocument()
    expect(screen.getByText('The recorded result needs attention.')).toBeInTheDocument()
  })

  test('explains why revoked context is still visible', () => {
    render(<ContextEvidenceList evidence={[]} revokedItems={[revokedItem()]} />)

    expect(screen.getByText('Old deployment memory')).toBeInTheDocument()
    expect(screen.getByText(/No longer used for future work/i)).toBeInTheDocument()
    expect(screen.getByText(/understand the past result/i)).toBeInTheDocument()
  })
})
