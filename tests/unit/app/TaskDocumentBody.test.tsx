import { render, screen } from '@testing-library/react'
import { describe, expect, test } from 'vitest'
import { TaskDocumentBody } from '@app/features/detail/document/TaskDocumentBody'

const base = {
  id: 't1',
  state: 'working',
  method: 'work',
  priority: 'normal',
  progress: 10,
  createdAt: new Date().toISOString(),
  updatedAt: new Date().toISOString(),
  attempt: 1,
} as const

describe('TaskDocumentBody', () => {
  test('renders the next-action callout and the brief as markdown', async () => {
    render(
      <TaskDocumentBody
        task={{ ...base, params: { task: 'T', message: '## Steps\n\n- one' } } as never}
      />
    )
    expect(screen.getByTestId('task-next-action')).toBeDefined()
    expect(await screen.findByRole('heading', { name: 'Steps' }, { timeout: 5_000 })).toBeDefined()
    expect(screen.getByRole('listitem')).toBeDefined()
  })

  test('falls back to beginner copy when the brief is empty', () => {
    render(<TaskDocumentBody task={{ ...base, params: { task: 'T', message: '' } } as never} />)
    expect(screen.getByTestId('task-brief-empty')).toBeDefined()
  })

  test('shows the handoff checklist only when completed', () => {
    const { rerender } = render(
      <TaskDocumentBody task={{ ...base, params: { task: 'T', message: 'm' } } as never} />
    )
    expect(screen.queryByTestId('task-handoff-checklist')).toBeNull()
    rerender(
      <TaskDocumentBody
        task={{ ...base, state: 'completed', params: { task: 'T', message: 'm' } } as never}
      />
    )
    expect(screen.getByTestId('task-handoff-checklist')).toBeDefined()
  })

  test('renders markdown results and keeps other artifacts preformatted', async () => {
    render(
      <TaskDocumentBody
        task={
          {
            ...base,
            params: { task: 'T', message: 'm' },
            result: [
              { name: 'summary.md', mimeType: 'text/markdown', data: '## Result notes' },
              { name: 'result.json', mimeType: 'application/json', data: '{"ok":true}' },
            ],
          } as never
        }
      />
    )
    expect(
      await screen.findByRole('heading', { name: 'Result notes' }, { timeout: 5_000 })
    ).toBeDefined()
    expect(screen.getByText('{"ok":true}').closest('pre')).not.toBeNull()
  })
})
