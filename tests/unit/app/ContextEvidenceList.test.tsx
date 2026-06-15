import { cleanup, fireEvent, render, screen } from '@testing-library/react'
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

    expect(screen.getByText('What the agent used')).toBeInTheDocument()
    expect(screen.getByText(/what the agent used or saved/i)).toBeInTheDocument()
    expect(screen.getByText('Task result')).toBeInTheDocument()
    expect(
      screen.getByText(/Final answer or status saved from the agent work/i)
    ).toBeInTheDocument()
    expect(screen.queryByText(/Final answer or status saved from the agent run/i)).toBeNull()
    expect(screen.getByText('Health check completed successfully.')).toBeInTheDocument()
    expect(
      screen.getByText(/Most users can rely on the summary above.*sharing details with support/i)
    ).toBeInTheDocument()
    expect(screen.queryByText(/sharing run details with support/i)).toBeNull()
    expect(screen.queryByText(/this run already used it/i)).toBeNull()
    expect(screen.getByText('Show support details')).toBeInTheDocument()
    expect(screen.queryByText('Evidence')).toBeNull()
    expect(screen.queryByText(/technical details/i)).toBeNull()
    expect(screen.queryByText(/raw details/i)).toBeNull()
  })

  test('describes tool evidence without API jargon', () => {
    render(
      <ContextEvidenceList
        evidence={[
          evidence({
            sourceType: 'tool_call',
            payload: { ok: true },
          }),
        ]}
        revokedItems={[]}
      />
    )

    expect(screen.getByText('Tool activity')).toBeInTheDocument()
    expect(
      screen.getByText('A recorded tool action that helped the agent complete the work.')
    ).toBeInTheDocument()
    expect(screen.queryByText(/API record/i)).toBeNull()
  })

  test('summarizes detailed payloads without implementation field wording', () => {
    render(
      <ContextEvidenceList
        evidence={[
          evidence({
            payload: { code: 'needs-review', retryable: true },
          }),
        ]}
        revokedItems={[]}
      />
    )

    expect(
      screen.getByText('Additional work details with 2 pieces of information.')
    ).toBeInTheDocument()
    expect(screen.queryByText('Additional run details with 2 pieces of information.')).toBeNull()
    expect(screen.queryByText(/Additional evidence/i)).toBeNull()
    expect(screen.queryByText(/fields/i)).toBeNull()
  })

  test('uses result-file wording for saved file evidence', () => {
    render(
      <ContextEvidenceList
        evidence={[
          evidence({
            sourceType: 'artifact',
            payload: { title: 'release-notes.md', description: 'Release notes saved for review.' },
          }),
        ]}
        revokedItems={[]}
      />
    )

    expect(screen.getByText('Saved result file')).toBeInTheDocument()
    expect(screen.getByText('A file or result saved while the task ran.')).toBeInTheDocument()
    expect(screen.queryByText('A file or result saved during the run.')).toBeNull()
    expect(screen.queryByText(/artifact/i)).toBeNull()
  })

  test('hides sensitive values in technical evidence details', () => {
    render(
      <ContextEvidenceList
        evidence={[
          evidence({
            payload: {
              ok: false,
              title: 'Deployment check',
              token: 'secret-token-value',
              nested: { apiKey: 'private-api-key', error: 'Missing token' },
            },
          }),
        ]}
        revokedItems={[]}
      />
    )

    expect(screen.getByText('Deployment check')).toBeInTheDocument()

    fireEvent.click(screen.getByText('Show support details'))

    expect(screen.getAllByText(/Hidden for safety/i).length).toBeGreaterThan(0)
    expect(screen.getByText(/Required account access is missing/i)).toBeInTheDocument()
    expect(screen.queryByText(/secret-token-value/i)).toBeNull()
    expect(screen.queryByText(/private-api-key/i)).toBeNull()
    expect(screen.queryByText(/Missing token/i)).toBeNull()
  })

  test('hides raw technical evidence failures from summaries and support details', () => {
    render(
      <ContextEvidenceList
        evidence={[
          evidence({
            payload: {
              ok: false,
              summary: 'panic: stack trace line 7 from raw command output',
              error: 'database unavailable: connection refused at postgres.internal:5432',
              nested: { stdout: 'secret token abc' },
            },
          }),
        ]}
        revokedItems={[]}
      />
    )

    expect(
      screen.getByText(
        'This record hit a problem. Ask the agent to explain what happened, then retry if the task still matters.'
      )
    ).toBeInTheDocument()
    expect(screen.queryByText(/panic/i)).toBeNull()
    expect(screen.queryByText(/stack trace/i)).toBeNull()
    expect(screen.queryByText(/raw command output/i)).toBeNull()

    fireEvent.click(screen.getByText('Show support details'))

    expect(screen.getAllByText(/hit a problem/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/technical problem/i)).toBeNull()
    expect(screen.getAllByText(/Hidden for safety/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/postgres\.internal/i)).toBeNull()
    expect(screen.queryByText(/connection refused/i)).toBeNull()
    expect(screen.queryByText(/secret token/i)).toBeNull()
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

    expect(screen.getByText('Work details')).toBeInTheDocument()
    expect(screen.getByText('Extra information recorded while the task ran.')).toBeInTheDocument()
    expect(screen.getByText('The recorded result needs attention.')).toBeInTheDocument()
    expect(screen.queryByText('Run details')).toBeNull()
    expect(screen.queryByText('Run evidence')).toBeNull()
    expect(screen.queryByText('Custom Probe')).toBeNull()
  })

  test('explains why revoked context is still visible', () => {
    render(<ContextEvidenceList evidence={[]} revokedItems={[revokedItem()]} />)

    expect(screen.getByText('Old deployment memory')).toBeInTheDocument()
    expect(screen.getByText(/No longer used for future work/i)).toBeInTheDocument()
    expect(screen.getByText(/because this task already used it/i)).toBeInTheDocument()
    expect(screen.getByText(/understand the past result/i)).toBeInTheDocument()
    expect(screen.queryByText(/because this run already used it/i)).toBeNull()
  })
})
