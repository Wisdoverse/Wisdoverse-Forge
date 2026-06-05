import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen, within } from '@testing-library/react'
import { userEvent } from '@testing-library/user-event'
import { ContextTab } from '@app/features/detail/ContextTab'
import type {
  AppliedContextItem,
  ContextFeedbackLabel,
  TaskContextResponse,
} from '@shared/types/context'

afterEach(cleanup)

const now = new Date('2026-05-06T07:00:00.000Z').toISOString()

function applied(overrides: Partial<AppliedContextItem>): AppliedContextItem {
  return {
    injectionId:
      overrides.injectionId ?? `inj-${overrides.itemKind ?? 'memory'}-${overrides.itemId}`,
    runId: overrides.runId ?? 'run-1',
    itemId: overrides.itemId ?? 'memory-1',
    itemKind: overrides.itemKind ?? 'memory',
    position: overrides.position ?? 0,
    title: overrides.title ?? 'Deployment memory',
    contentPreview: overrides.contentPreview ?? 'Use prod-ext before merging.',
    contentTruncated: overrides.contentTruncated ?? false,
    contentRef: overrides.contentRef ?? 'memory_items/memory-1',
    scopeKind: overrides.scopeKind ?? 'project',
    scopeId: overrides.scopeId ?? 'project-1',
    sensitivity: overrides.sensitivity ?? 'internal',
    state: overrides.state ?? 'active',
    revoked: overrides.revoked ?? false,
    sourceTaskId: overrides.sourceTaskId ?? 'task-source-1',
    sourceRunId: overrides.sourceRunId ?? null,
    source: overrides.source ?? {
      sourceType: 'memory_item',
      sourceId: overrides.itemId ?? 'memory-1',
      title: overrides.title ?? 'Deployment memory',
    },
    lastUsedAt: overrides.lastUsedAt ?? now,
    lastVerifiedAt: overrides.lastVerifiedAt ?? now,
    appliedAt: overrides.appliedAt ?? now,
    adapter: overrides.adapter ?? 'claude',
    envelopeVersion: overrides.envelopeVersion ?? 'v1',
    capabilityProfile: overrides.capabilityProfile ?? {},
    degradationReason: overrides.degradationReason ?? null,
    feedback: overrides.feedback ?? null,
  }
}

function context(overrides: Partial<TaskContextResponse> = {}): TaskContextResponse {
  return {
    taskId: 'task-1',
    runs: [
      {
        id: 'run-1',
        status: 'completed',
        agentId: 'agent-1',
        startedAt: now,
        finishedAt: now,
        capabilityProfile: {},
      },
    ],
    appliedItems: [],
    suggestedMemoryUpdates: [],
    skillCandidates: [],
    evidence: [],
    provenance: [],
    ...overrides,
  }
}

describe('ContextTab', () => {
  test('shows beginner guidance when task context fails to load', async () => {
    render(
      <ContextTab
        taskId="task-1"
        loadContext={async () => {
          throw new Error('401 Unauthorized')
        }}
      />
    )

    expect(await screen.findByText(/sign in again/i)).toBeDefined()
    expect(screen.queryByText(/code: 401/i)).toBeNull()
    expect(screen.queryByText(/401 unauthorized/i)).toBeNull()
  })

  test('shows the empty state when no run context exists', async () => {
    render(<ContextTab taskId="task-1" loadContext={async () => context({ runs: [] })} />)

    const emptyState = await screen.findByTestId('context-empty-state')
    expect(within(emptyState).getByText('No context has been applied yet')).toBeDefined()
    expect(
      within(emptyState).getByText(/Context appears here after an agent run uses saved memories/i)
    ).toBeDefined()
    expect(
      within(emptyState).getByText(/Publish or run the task so Forge can choose memories/i)
    ).toBeDefined()
    expect(within(emptyState).getByText(/Use feedback on applied items/i)).toBeDefined()
  })

  test('shows beginner recovery guidance when task context fails to load', async () => {
    render(
      <ContextTab
        taskId="task-1"
        loadContext={async () => {
          throw new Error('HTTP 403')
        }}
      />
    )

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent(
      'You do not have permission to view this task. Ask an owner or admin to update your role.'
    )
    expect(alert).not.toHaveTextContent('HTTP 403')
  })

  test('renders applied context, candidates, evidence, and provenance', async () => {
    const memory = applied({
      itemId: 'memory-1',
      title: 'Prod deploy memory',
      degradationReason: 'source snapshot was shortened',
    })
    const skill = applied({
      itemId: 'skill-1',
      itemKind: 'skill',
      title: 'Release checklist',
      contentPreview: 'Check migrations and health probes.',
      scopeKind: 'org',
      contentRef: 'skills/skill-1',
    })
    const revoked = applied({
      itemId: 'memory-2',
      title: 'Old deploy path',
      state: 'revoked',
      revoked: true,
      position: 2,
    })

    render(
      <ContextTab
        taskId="task-1"
        loadContext={async () =>
          context({
            appliedItems: [memory, skill, revoked],
            suggestedMemoryUpdates: [
              {
                id: 'candidate-1',
                itemKind: 'memory',
                state: 'pending',
                ownerUserId: 'user-1',
                sourceRunId: 'run-1',
                proposedPreview: {
                  title: 'New release memory',
                  content_preview: 'Persist the new production release note.',
                },
                createdAt: now,
                updatedAt: now,
              },
            ],
            skillCandidates: [
              {
                id: 'candidate-2',
                itemKind: 'skill',
                state: 'pending',
                ownerUserId: 'user-1',
                sourceRunId: 'run-1',
                targetSkillId: 'skill-1',
                proposedPreview: {
                  name: 'Release operator',
                  content_preview: 'Run the validated release workflow.',
                },
                createdAt: now,
                updatedAt: now,
              },
            ],
            evidence: [
              {
                runId: 'run-1',
                sourceType: 'task_result',
                sourceId: 'result-1',
                payload: { ok: true },
                createdAt: now,
              },
            ],
            provenance: [
              {
                runId: 'run-1',
                itemId: 'memory-1',
                itemKind: 'memory',
                title: 'Prod deploy memory',
                source: {
                  sourceType: 'memory_item',
                  sourceId: 'memory-1',
                  title: 'Prod deploy memory',
                },
                adapter: 'claude',
                envelopeVersion: 'v1',
                appliedAt: now,
                state: 'active',
                revoked: false,
              },
            ],
          })
        }
      />
    )

    expect(await screen.findByTestId('context-tab')).toBeDefined()
    expect(screen.getByText('Agent work checked for context')).toBeDefined()
    expect(screen.getByText('Work run 1')).toBeDefined()
    expect(screen.getByText('Finished')).toBeDefined()
    expect(screen.getByText('Applied memories')).toBeDefined()
    expect(screen.getAllByText(/added to the agent's working context/i).length).toBeGreaterThan(0)
    expect(screen.getAllByText('Prod deploy memory').length).toBeGreaterThan(0)
    expect(screen.getByText(/limited context: source snapshot was shortened/i)).toBeDefined()
    expect(screen.getByText('Applied skills')).toBeDefined()
    expect(screen.getByText('Release checklist')).toBeDefined()
    expect(screen.getByText('Suggested memory updates')).toBeDefined()
    expect(screen.getByText('New release memory')).toBeDefined()
    expect(screen.getByText('Suggested skills to review')).toBeDefined()
    expect(screen.getByText('Release operator')).toBeDefined()
    expect(screen.getByTestId('context-evidence')).toBeDefined()
    expect(screen.getByText(/No longer used for future work/)).toBeDefined()
    expect(screen.getByTestId('context-provenance')).toBeDefined()
    expect(screen.getByText('Where saved context came from')).toBeDefined()
    expect(
      screen.getByText(/came from Prod deploy memory and was used during this agent run/i)
    ).toBeDefined()
    expect(screen.queryByText(/via claude/i)).toBeNull()
    expect(screen.queryByText(/envelope/i)).toBeNull()
  })

  test('loads full memory content only after Show more is clicked', async () => {
    const readMemoryContent = vi.fn(async () => ({
      id: 'memory-1',
      content: 'Full memory content loaded on demand.',
      content_redacted: false,
      sensitivity: 'internal' as const,
    }))

    render(
      <ContextTab
        taskId="task-1"
        loadContext={async () =>
          context({
            appliedItems: [
              applied({
                itemId: 'memory-1',
                contentPreview: 'Short preview...',
                contentTruncated: true,
              }),
            ],
          })
        }
        readMemoryContent={readMemoryContent}
      />
    )

    expect(await screen.findByText('Short preview...')).toBeDefined()
    expect(readMemoryContent).not.toHaveBeenCalled()

    await userEvent.setup().click(screen.getByRole('button', { name: /show full memory/i }))

    expect(readMemoryContent).toHaveBeenCalledWith('memory-1')
    expect(await screen.findByText('Full memory content loaded on demand.')).toBeDefined()
  })

  test('shows a beginner-safe message when full memory content fails to load', async () => {
    const readMemoryContent = vi.fn(async () => {
      throw new Error('raw backend failure')
    })

    render(
      <ContextTab
        taskId="task-1"
        loadContext={async () =>
          context({
            appliedItems: [
              applied({
                itemId: 'memory-1',
                contentPreview: 'Short preview...',
                contentTruncated: true,
              }),
            ],
          })
        }
        readMemoryContent={readMemoryContent}
      />
    )

    await screen.findByText('Short preview...')
    await userEvent.setup().click(screen.getByRole('button', { name: /show full memory/i }))

    expect(await screen.findByRole('alert')).toHaveTextContent(/full context could not load/i)
    expect(screen.queryByText('raw backend failure')).toBeNull()
  })

  test('submits feedback without a page reload', async () => {
    const recordFeedback = vi.fn(
      async (_item: AppliedContextItem, label: ContextFeedbackLabel) =>
        ({
          feedback: {
            id: 'feedback-1',
            organization_id: 'org-1',
            workspace_id: 'workspace-1',
            run_id: 'run-1',
            item_id: 'memory-1',
            item_kind: 'memory',
            label,
            note: null,
            user_id: 'user-1',
            created_at: now,
            updated_at: now,
          },
          item_state_changed: false,
        }) as const
    )

    render(
      <ContextTab
        taskId="task-1"
        loadContext={async () => context({ appliedItems: [applied({ itemId: 'memory-1' })] })}
        recordFeedback={recordFeedback}
      />
    )

    const card = await screen.findByText('Deployment memory')
    const article = card.closest('article')
    expect(article).not.toBeNull()
    await userEvent.setup().click(within(article!).getByRole('button', { name: 'Useful' }))

    expect(recordFeedback).toHaveBeenCalledOnce()
    expect(recordFeedback.mock.calls[0][1]).toBe('useful')
  })
})
