import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, test } from 'vitest'
import { InjectionPreviewModal } from '@app/entities/context/ui/InjectionPreviewModal'
import type { ContextPreviewResponse } from '@shared/types/context'

afterEach(cleanup)

const preview: ContextPreviewResponse = {
  contextPreviewId: 'preview-1',
  previewHash: 'hash-1',
  taskId: 'task-1',
  agentId: 'agent-1',
  expiresAt: '2026-04-25T06:30:00Z',
  capability: { cli_tool: 'codex', runtime_kind: 'container' },
  degradation: [],
  items: [
    {
      id: 'item-1',
      itemKind: 'memory',
      title: 'Deploy checklist',
      selected: true,
      pinned: false,
      estimatedTokens: 120,
      why: 'This note explains the deploy checks to repeat.',
    },
  ],
  suggestedItems: [],
  previouslyPinned: [],
  warnings: [],
}

describe('InjectionPreviewModal', () => {
  test('uses send wording for the task context review', () => {
    render(
      <InjectionPreviewModal isOpen preview={preview} onClose={() => {}} onConfirm={() => {}} />
    )

    expect(screen.getByText('Review saved notes before sending')).toBeDefined()
    expect(
      screen.getByText('Checked items will be shared with the agent when you send the task.')
    ).toBeDefined()
    expect(screen.getByRole('button', { name: 'Send task with selected notes' })).toBeDefined()
    expect(screen.queryByText(/publish/i)).toBeNull()
    expect(screen.queryByText(/selected context/i)).toBeNull()
  })
})
