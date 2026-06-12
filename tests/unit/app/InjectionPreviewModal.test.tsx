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
  capability: { cli_tool: 'codex', runtime_kind: 'container', max_context_tokens: 4000 },
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
  test('uses send wording for the saved notes review', () => {
    render(
      <InjectionPreviewModal isOpen preview={preview} onClose={() => {}} onConfirm={() => {}} />
    )

    expect(screen.getByText('Review saved notes before sending')).toBeDefined()
    expect(
      screen.getByText('Checked items will be shared with the agent when you send the task.')
    ).toBeDefined()
    expect(screen.getByRole('button', { name: 'Send task with selected notes' })).toBeDefined()
    expect(screen.getAllByLabelText('Close saved notes review')).toHaveLength(2)
    expect(screen.queryByText(/publish/i)).toBeNull()
    expect(screen.queryByText(/selected context/i)).toBeNull()
  })

  test('explains note space without context-unit jargon', () => {
    render(
      <InjectionPreviewModal isOpen preview={preview} onClose={() => {}} onConfirm={() => {}} />
    )

    expect(screen.getByTestId('context-fit-summary').textContent).toContain(
      "Fits in this agent's note space (4,000 units available)"
    )
    expect(screen.getByText('Note limits')).toBeDefined()
    expect(screen.getByText('No note limits right now')).toBeDefined()
    expect(screen.getByText('Uses about 120 units of note space')).toBeDefined()
    expect(screen.getByLabelText('Remove Deploy checklist from this task')).toBeDefined()
    expect(screen.queryByText(new RegExp(['context', 'units'].join('\\s+'), 'i'))).toBeNull()
    expect(screen.queryByText(new RegExp(['Limits', 'applied'].join('\\s+'), 'i'))).toBeNull()
  })

  test('uses plain note-limit wording when saved notes are shortened', () => {
    render(
      <InjectionPreviewModal
        isOpen
        preview={{ ...preview, degradation: ['budget_truncated'] }}
        onClose={() => {}}
        onConfirm={() => {}}
      />
    )

    expect(
      screen.getByText('Some notes will be left out because this agent has limited note space')
    ).toBeDefined()
    expect(
      screen.queryByText(new RegExp(['limited', 'context', 'room'].join('\\s+'), 'i'))
    ).toBeNull()
  })

  test('uses plain saved-notes wording for loading and empty states', () => {
    render(
      <InjectionPreviewModal
        isOpen
        preview={null}
        loading
        onClose={() => {}}
        onConfirm={() => {}}
      />
    )

    expect(screen.getByText('Loading saved notes review…')).toBeDefined()
    expect(screen.queryByText(new RegExp(['Loading', 'context', 'review'].join('\\s+')))).toBeNull()

    cleanup()

    render(<InjectionPreviewModal isOpen preview={null} onClose={() => {}} onConfirm={() => {}} />)

    expect(screen.getByText('No saved notes review is available yet.')).toBeDefined()
    expect(screen.queryByText(new RegExp(['No', 'context', 'review'].join('\\s+')))).toBeNull()
  })

  test('describes unknown saved items and helper-agent limits without jargon', () => {
    render(
      <InjectionPreviewModal
        isOpen
        preview={{
          ...preview,
          degradation: ['no_subagents'],
          items: [
            {
              ...preview.items[0],
              itemKind: 'future_item_kind' as never,
            },
          ],
        }}
        onClose={() => {}}
        onConfirm={() => {}}
      />
    )

    expect(screen.getByText('Saved item needs review')).toBeDefined()
    expect(screen.getByText('Notes meant only for helper agents will be skipped')).toBeDefined()
    expect(screen.queryByText(new RegExp(['Subagent-specific', 'context'].join('\\s+')))).toBeNull()
    expect(
      screen.queryByText(new RegExp(['Context', 'item', 'needs', 'review'].join('\\s+')))
    ).toBeNull()
  })
})
