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

    expect(screen.getByText('What helped produce this result')).toBeInTheDocument()
    expect(
      screen.getByText(/details show the answers, steps, and files used or saved/i)
    ).toBeInTheDocument()
    expect(screen.getByText('Final answer')).toBeInTheDocument()
    expect(
      screen.getByText(/agent's final answer or saved status for this task/i)
    ).toBeInTheDocument()
    expect(screen.queryByText('Task result')).toBeNull()
    expect(screen.queryByText(/Final answer or status saved from the agent work/i)).toBeNull()
    expect(screen.queryByText(/Final answer or status saved from the agent run/i)).toBeNull()
    expect(screen.getByText('Health check completed successfully.')).toBeInTheDocument()
    expect(
      screen.getByText(
        /Most users can rely on the summary above.*Open saved details.*sharing details with an owner or admin/i
      )
    ).toBeInTheDocument()
    expect(screen.queryByText(/Open the full record/i)).toBeNull()
    expect(screen.queryByText(/These records show/i)).toBeNull()
    expect(screen.queryByText(/^Recorded /i)).toBeNull()
    expect(screen.queryByText(/sharing run details with support/i)).toBeNull()
    expect(screen.queryByText(/support details/i)).toBeNull()
    expect(screen.queryByText(/details with support/i)).toBeNull()
    expect(screen.queryByText(/this run already used it/i)).toBeNull()
    expect(screen.getByText('Show saved details')).toBeInTheDocument()
    expect(screen.queryByText('Show full record')).toBeNull()
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

    expect(screen.getByText('Step the agent took')).toBeInTheDocument()
    expect(screen.getByText('An action the agent took to complete the work.')).toBeInTheDocument()
    expect(screen.queryByText('Tool activity')).toBeNull()
    expect(screen.queryByText(/recorded tool action/i)).toBeNull()
    expect(screen.queryByText(/API record/i)).toBeNull()
  })

  test('names source-message evidence as a message used for the work', () => {
    render(
      <ContextEvidenceList
        evidence={[
          evidence({
            sourceType: 'source_message',
            payload: { summary: 'The request asked for a production-ready review.' },
          }),
        ]}
        revokedItems={[]}
      />
    )

    expect(screen.getByText('Message used for this work')).toBeInTheDocument()
    expect(
      screen.getByText('A message the agent used while preparing the result.')
    ).toBeInTheDocument()
    expect(screen.queryByText('Source message')).toBeNull()
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
    expect(
      screen.getByText('A file or result saved while the work was running.')
    ).toBeInTheDocument()
    expect(screen.queryByText('A file or result saved while the task ran.')).toBeNull()
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

    fireEvent.click(screen.getByText('Show saved details'))

    expect(screen.getAllByText(/Hidden for safety/i).length).toBeGreaterThan(0)
    expect(screen.getAllByText(/Required account access is missing/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/secret-token-value/i)).toBeNull()
    expect(screen.queryByText(/private-api-key/i)).toBeNull()
    expect(screen.queryByText(/Missing token/i)).toBeNull()
  })

  test('turns reversed expired access errors into reconnect guidance', () => {
    render(
      <ContextEvidenceList
        evidence={[
          evidence({
            payload: {
              ok: false,
              summary: 'token expired',
              error: 'credential expired',
            },
          }),
        ]}
        revokedItems={[]}
      />
    )

    expect(screen.getAllByText(/Required account access is missing/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/token expired/i)).toBeNull()

    fireEvent.click(screen.getByText('Show saved details'))

    expect(screen.getAllByText(/Required account access is missing/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/credential expired/i)).toBeNull()
  })

  test('turns revoked access errors into reconnect guidance', () => {
    render(
      <ContextEvidenceList
        evidence={[
          evidence({
            payload: {
              ok: false,
              summary: 'token revoked',
              error: 'revoked credential',
            },
          }),
        ]}
        revokedItems={[]}
      />
    )

    expect(screen.getAllByText(/Required account access is missing/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/token revoked/i)).toBeNull()

    fireEvent.click(screen.getByText('Show saved details'))

    expect(screen.getAllByText(/Required account access is missing/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/revoked credential/i)).toBeNull()
  })

  test('hides camelCase access token values in saved details', () => {
    render(
      <ContextEvidenceList
        evidence={[
          evidence({
            payload: {
              title: 'Tool access check',
              accessToken: 'secret-camel-token-value',
            },
          }),
        ]}
        revokedItems={[]}
      />
    )

    fireEvent.click(screen.getByText('Show saved details'))

    expect(screen.getByText(/Hidden for safety/i)).toBeInTheDocument()
    expect(screen.queryByText(/secret-camel-token-value/i)).toBeNull()
    expect(screen.queryByText(/accessToken/i)).toBeNull()
  })

  test('hides prefixed api key values in saved details', () => {
    render(
      <ContextEvidenceList
        evidence={[
          evidence({
            payload: {
              title: 'Tool access check',
              xApiKey: 'secret-prefixed-api-key-value',
            },
          }),
        ]}
        revokedItems={[]}
      />
    )

    fireEvent.click(screen.getByText('Show saved details'))

    expect(screen.getByText(/Hidden for safety/i)).toBeInTheDocument()
    expect(screen.queryByText(/secret-prefixed-api-key-value/i)).toBeNull()
    expect(screen.queryByText(/xApiKey/i)).toBeNull()
  })

  test('hides bearer authorization text in saved details', () => {
    render(
      <ContextEvidenceList
        evidence={[
          evidence({
            payload: {
              title: 'Request details',
              headers: 'Authorization: Bearer saved-secret-token',
            },
          }),
        ]}
        revokedItems={[]}
      />
    )

    fireEvent.click(screen.getByText('Show saved details'))

    expect(screen.getByText(/Hidden for safety/i)).toBeInTheDocument()
    expect(screen.queryByText(/Bearer saved-secret-token/i)).toBeNull()
    expect(screen.queryByText(/Authorization/i)).toBeNull()
  })

  test('turns technical saved-detail summaries into plain next steps', () => {
    render(
      <ContextEvidenceList
        evidence={[
          evidence({
            payload: {
              ok: false,
              summary: 'HTTP 500 from provider payload validation endpoint',
              payload: 'raw payload shape mismatch',
              provider: 'internal-provider-name',
            },
          }),
        ]}
        revokedItems={[]}
      />
    )

    expect(
      screen.getByText(
        'Behind-the-scenes details were hidden for safety. Check the summary above, then ask the agent to explain what happened if the task still matters.'
      )
    ).toBeInTheDocument()
    expect(screen.queryByText(/HTTP 500/i)).toBeNull()
    expect(screen.queryByText(/provider/i)).toBeNull()
    expect(screen.queryByText(/payload/i)).toBeNull()

    fireEvent.click(screen.getByText('Show saved details'))

    expect(screen.getAllByText(/Behind-the-scenes details were hidden/i).length).toBeGreaterThan(0)
    expect(screen.getByText(/Status: needs checking/i)).toBeInTheDocument()
    expect(screen.queryByText(/raw payload/i)).toBeNull()
    expect(screen.queryByText(/internal-provider-name/i)).toBeNull()
  })

  test('hides raw technical evidence failures from summaries and full records', () => {
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
        'Behind-the-scenes details were hidden for safety. Check the summary above, then ask the agent to explain what happened if the task still matters.'
      )
    ).toBeInTheDocument()
    expect(screen.queryByText(/panic/i)).toBeNull()
    expect(screen.queryByText(/stack trace/i)).toBeNull()
    expect(screen.queryByText(/raw command output/i)).toBeNull()

    fireEvent.click(screen.getByText('Show saved details'))

    expect(screen.getAllByText(/Behind-the-scenes details were hidden/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/technical problem/i)).toBeNull()
    expect(screen.getAllByText(/Hidden for safety/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/postgres\.internal/i)).toBeNull()
    expect(screen.queryByText(/connection refused/i)).toBeNull()
    expect(screen.queryByText(/secret token/i)).toBeNull()
  })

  test('uses saved-detail wording when a full record cannot be shown safely', () => {
    const circularPayload: Record<string, unknown> = {}
    circularPayload.self = circularPayload

    render(
      <ContextEvidenceList
        evidence={[
          evidence({
            payload: circularPayload,
          }),
        ]}
        revokedItems={[]}
      />
    )

    fireEvent.click(screen.getByText('Show saved details'))

    expect(screen.getByText(/Saved details could not be shown safely/i)).toBeDefined()
    expect(screen.getByText(/Check the summary above/i)).toBeDefined()
    expect(screen.queryByText(/Saved details were recorded/i)).toBeNull()
    expect(screen.queryByText(/Review the summary above/i)).toBeNull()
    expect(screen.queryByText(/Full record details/i)).toBeNull()
  })

  test('explains empty saved-detail values without unavailable dead ends', () => {
    render(
      <ContextEvidenceList
        evidence={[
          evidence({
            payload: {
              title: 'Release check',
              result: null,
            },
          }),
        ]}
        revokedItems={[]}
      />
    )

    fireEvent.click(screen.getByText('Show saved details'))

    expect(screen.getByText(/Saved detail: not saved for this item/i)).toBeInTheDocument()
    expect(screen.queryByText(/not available/i)).toBeNull()
    expect(screen.queryByText(/retry if needed/i)).toBeNull()
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
    expect(
      screen.getByText('Extra information saved while the work was running.')
    ).toBeInTheDocument()
    expect(screen.queryByText('Extra information recorded while the task ran.')).toBeNull()
    expect(screen.getByText('Check the saved result before reusing it.')).toBeInTheDocument()
    expect(screen.queryByText('Check the recorded result before reusing it.')).toBeNull()
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
