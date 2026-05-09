import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen, within } from '@testing-library/react'
import { userEvent } from '@testing-library/user-event'
import { InjectionPreviewModal } from '@app/entities/context/ui/InjectionPreviewModal'
import type { ContextPreviewResponse } from '@shared/types/context'

afterEach(cleanup)

const now = '2026-05-06T09:00:00.000Z'

function preview(overrides: Partial<ContextPreviewResponse> = {}): ContextPreviewResponse {
  return {
    contextPreviewId: 'preview-1',
    previewHash: 'hash-1',
    taskId: 'task-1',
    agentId: 'agent-1',
    expiresAt: now,
    capability: {
      cli_tool: 'claude',
      runtime_kind: 'container',
      max_context_tokens: 1200,
    },
    degradation: [],
    items: [
      {
        id: 'memory-1',
        itemKind: 'memory',
        title: 'Prod deploy memory',
        selected: true,
        pinned: false,
        scopeKind: 'project',
        scopeId: 'project-1',
        sensitivity: 'internal',
        estimatedTokens: 120,
        lastUsedAt: now,
        lastVerifiedAt: now,
        why: 'Matched task text.',
      },
      {
        id: 'memory-2',
        itemKind: 'memory',
        title: 'Release rollback memory',
        selected: true,
        pinned: false,
        scopeKind: 'team',
        scopeId: 'team-1',
        sensitivity: 'confidential',
        estimatedTokens: 80,
        lastUsedAt: null,
        lastVerifiedAt: now,
        why: 'Recent useful feedback.',
      },
    ],
    suggestedItems: [
      {
        id: 'memory-3',
        itemKind: 'memory',
        title: 'Pinned migration note',
        selected: false,
        pinned: false,
        scopeKind: 'project',
        scopeId: 'project-1',
        sensitivity: 'internal',
        estimatedTokens: 300,
        lastUsedAt: null,
        lastVerifiedAt: null,
        why: 'Outside the default context budget.',
      },
    ],
    previouslyPinned: [],
    warnings: [],
    ...overrides,
  }
}

describe('InjectionPreviewModal', () => {
  test('renders capability summary and preview sections', () => {
    render(
      <InjectionPreviewModal
        isOpen
        preview={preview({ degradation: ['budget_truncated'] })}
        onClose={() => {}}
        onConfirm={() => {}}
      />
    )

    expect(screen.getByRole('dialog', { name: 'Context injection preview' })).toBeDefined()
    expect(screen.getByText('claude')).toBeDefined()
    expect(screen.getByText('container')).toBeDefined()
    expect(screen.getByText('budget_truncated')).toBeDefined()
    expect(screen.getByText('Items to inject')).toBeDefined()
    expect(screen.getByText('Suggested but unselected')).toBeDefined()
    expect(screen.getByText('Previously pinned')).toBeDefined()
  })

  test('submits removed default items and pinned suggested items', async () => {
    const onConfirm = vi.fn()
    render(
      <InjectionPreviewModal isOpen preview={preview()} onClose={() => {}} onConfirm={onConfirm} />
    )

    await userEvent
      .setup()
      .click(screen.getByRole('checkbox', { name: 'Select Release rollback memory' }))
    const suggested = screen.getByText('Pinned migration note').closest('div')
    expect(suggested).not.toBeNull()
    await userEvent.setup().click(screen.getByRole('button', { name: 'Pin Pinned migration note' }))
    await userEvent.setup().click(screen.getByRole('button', { name: 'Publish with context' }))

    expect(onConfirm).toHaveBeenCalledWith({
      pinnedIds: ['memory-3'],
      removedIds: ['memory-2'],
    })
  })

  test('cancel closes without publishing', async () => {
    const onClose = vi.fn()
    const onConfirm = vi.fn()
    render(
      <InjectionPreviewModal isOpen preview={preview()} onClose={onClose} onConfirm={onConfirm} />
    )

    await userEvent
      .setup()
      .click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Cancel' }))

    expect(onClose).toHaveBeenCalledOnce()
    expect(onConfirm).not.toHaveBeenCalled()
  })
})
