import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { userEvent } from '@testing-library/user-event'
import { FeedbackControls } from '@app/entities/context/ui/FeedbackControls'
import type {
  AppliedContextItem,
  ContextFeedbackLabel,
  ContextFeedbackOutcome,
} from '@shared/types/context'

afterEach(cleanup)

const now = '2026-05-25T10:00:00.000Z'

function contextItem(overrides: Partial<AppliedContextItem> = {}): AppliedContextItem {
  return {
    injectionId: 'injection-1',
    runId: 'run-1',
    itemId: 'memory-1',
    itemKind: 'memory',
    position: 0,
    title: 'Release memory',
    contentPreview: 'Use the validated release checklist.',
    contentTruncated: false,
    contentRef: 'memory_items/memory-1',
    scopeKind: 'project',
    scopeId: 'project-1',
    sensitivity: 'internal',
    state: 'active',
    revoked: false,
    sourceTaskId: 'task-1',
    sourceRunId: 'run-1',
    source: {
      sourceType: 'memory_item',
      sourceId: 'memory-1',
      title: 'Release memory',
    },
    lastUsedAt: now,
    lastVerifiedAt: now,
    appliedAt: now,
    adapter: 'claude',
    envelopeVersion: 'v1',
    capabilityProfile: {},
    degradationReason: null,
    feedback: null,
    ...overrides,
  }
}

function outcome(label: ContextFeedbackLabel): ContextFeedbackOutcome {
  return {
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
  }
}

describe('FeedbackControls', () => {
  test('explains context feedback choices in beginner language', () => {
    render(<FeedbackControls item={contextItem()} onRecord={async (label) => outcome(label)} />)

    expect(screen.getByText('Was this context helpful?')).toBeInTheDocument()
    expect(
      screen.getByText('Your answer helps future runs choose safer, more useful context.')
    ).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Outdated' })).toHaveAttribute(
      'title',
      'The information is old and should be checked before reuse.'
    )
    expect(screen.getByRole('button', { name: 'Do not use again' })).toHaveAttribute(
      'title',
      'Stop selecting this item for future runs.'
    )
  })

  test('records feedback and confirms what future runs will do', async () => {
    const onRecord = vi.fn(async (label: ContextFeedbackLabel) => outcome(label))

    render(<FeedbackControls item={contextItem()} onRecord={onRecord} />)

    await userEvent.setup().click(screen.getByRole('button', { name: 'Too sensitive' }))

    expect(onRecord).toHaveBeenCalledWith('too_sensitive')
    expect(
      await screen.findByText('Saved: future runs will handle this item more carefully.')
    ).toBeInTheDocument()
  })

  test('shows recovery guidance when feedback cannot be saved', async () => {
    const onRecord = vi.fn(async () => {
      throw new Error('API 403: Forbidden')
    })

    render(<FeedbackControls item={contextItem()} onRecord={onRecord} />)

    await userEvent.setup().click(screen.getByRole('button', { name: 'Outdated' }))

    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain('You do not have permission')
    expect(alert.textContent).toContain('Ask an owner or admin')
    expect(alert.textContent).not.toContain('API 403')
    expect(alert.textContent).not.toContain('Forbidden')
    expect(screen.getByRole('button', { name: 'Outdated' })).not.toHaveClass('bg-apple-blue')
  })
})
