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
  test('renders a beginner-readable review before publishing context', () => {
    render(
      <InjectionPreviewModal
        isOpen
        preview={preview({ degradation: ['budget_truncated'] })}
        onClose={() => {}}
        onConfirm={() => {}}
      />
    )

    expect(screen.getByRole('dialog', { name: 'Review context before publishing' })).toBeDefined()
    expect(
      screen.getByText(/saved notes and skill instructions the agent will see next/i)
    ).toBeDefined()
    expect(
      screen.getByText(
        "2 items selected · Fits in this agent's context (1,200 context units available)"
      )
    ).toBeDefined()
    expect(screen.getByText('Agent will use')).toBeDefined()
    expect(screen.getByText('Claude')).toBeDefined()
    expect(screen.getByText('Work location')).toBeDefined()
    expect(screen.getByText('Managed workspace')).toBeDefined()
    expect(screen.getByText('Limits applied')).toBeDefined()
    expect(
      screen.getByText('Some notes will be left out because this agent has limited context room')
    ).toBeDefined()
    expect(screen.getByText('Will be included')).toBeDefined()
    expect(
      screen.getByText('Checked items will be shared with the agent when you publish.')
    ).toBeDefined()
    expect(screen.getByText('Optional matches')).toBeDefined()
    expect(
      screen.getByText('These may help, but they stay out unless you choose them.')
    ).toBeDefined()
    expect(screen.getByText('Pinned for later')).toBeDefined()
    expect(screen.getAllByText('Saved note').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Project').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Internal').length).toBeGreaterThan(0)
    expect(screen.getByText('Needs about 120 context units')).toBeDefined()
  })

  test('uses chat-only AI service wording for provider context reviews', () => {
    render(
      <InjectionPreviewModal
        isOpen
        preview={preview({
          capability: {
            runtime_kind: 'provider',
            max_context_tokens: 1200,
          },
        })}
        onClose={() => {}}
        onConfirm={() => {}}
      />
    )

    expect(screen.getByText('Work location')).toBeDefined()
    expect(screen.getByText('Chat-only AI service')).toBeDefined()
    expect(screen.queryByText(/Text-only model/i)).toBeNull()
  })

  test('labels unknown work locations without exposing backend values', () => {
    render(
      <InjectionPreviewModal
        isOpen
        preview={preview({
          capability: {
            cli_tool: 'codex',
            runtime_kind: 'future_runtime' as never,
            max_context_tokens: 1200,
          },
        })}
        onClose={() => {}}
        onConfirm={() => {}}
      />
    )

    expect(screen.getByText('Work location')).toBeDefined()
    expect(screen.getByText('Work location needs review')).toBeDefined()
    expect(screen.queryByText(/future runtime/i)).toBeNull()
    expect(screen.queryByText('Unknown')).toBeNull()
  })

  test('labels missing work locations as not listed', () => {
    render(
      <InjectionPreviewModal
        isOpen
        preview={preview({
          capability: {
            cli_tool: 'codex',
            max_context_tokens: 1200,
          },
        })}
        onClose={() => {}}
        onConfirm={() => {}}
      />
    )

    expect(screen.getByText('Work location')).toBeDefined()
    expect(screen.getByText('Work location not listed')).toBeDefined()
    expect(screen.queryByText('Runtime')).toBeNull()
  })

  test('labels unknown context badges without exposing backend values', () => {
    render(
      <InjectionPreviewModal
        isOpen
        preview={preview({
          degradation: ['future_limit_reason'],
          items: [
            {
              id: 'memory-unknown-badges',
              itemKind: 'future_context_kind' as never,
              title: 'Unknown badge memory',
              selected: true,
              pinned: false,
              scopeKind: 'global_workspace' as never,
              scopeId: 'workspace-1',
              sensitivity: 'restricted_zone' as never,
              estimatedTokens: 90,
              lastUsedAt: null,
              lastVerifiedAt: null,
              why: 'Matched task text.',
            },
          ],
          suggestedItems: [],
        })}
        onClose={() => {}}
        onConfirm={() => {}}
      />
    )

    expect(screen.getByText('Unknown badge memory')).toBeDefined()
    expect(screen.getByText('Context item needs review')).toBeDefined()
    expect(screen.getByText('Scope needs review')).toBeDefined()
    expect(screen.getByText('Sensitivity needs review')).toBeDefined()
    expect(screen.getByText('Some context limits need review')).toBeDefined()
    expect(screen.queryByText(/future context kind/i)).toBeNull()
    expect(screen.queryByText(/future limit reason/i)).toBeNull()
    expect(screen.queryByText(/global workspace/i)).toBeNull()
    expect(screen.queryByText(/restricted zone/i)).toBeNull()
    expect(screen.queryByText('Unknown')).toBeNull()
  })

  test('submits removed default items and pinned suggested items', async () => {
    const onConfirm = vi.fn()
    render(
      <InjectionPreviewModal isOpen preview={preview()} onClose={() => {}} onConfirm={onConfirm} />
    )

    await userEvent
      .setup()
      .click(screen.getByRole('checkbox', { name: 'Remove Release rollback memory from context' }))
    const suggested = screen.getByText('Pinned migration note').closest('div')
    expect(suggested).not.toBeNull()
    await userEvent
      .setup()
      .click(screen.getByRole('button', { name: 'Keep Pinned migration note pinned' }))
    await userEvent
      .setup()
      .click(screen.getByRole('button', { name: 'Publish with selected context' }))

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
