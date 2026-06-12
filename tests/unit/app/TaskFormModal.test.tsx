import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { TaskFormModal, type TaskProjectOption } from '@app/features/board/TaskFormModal'

const PROJECTS: TaskProjectOption[] = [
  { id: 'p1', name: 'Platform', teamId: 't1', teamName: 'Core', color: '#007AFF' },
]

function renderModal(onSubmit = vi.fn()) {
  render(
    <TaskFormModal
      isOpen
      onClose={vi.fn()}
      onSubmit={onSubmit}
      projects={PROJECTS}
      selectedProjectId="p1"
      selectedTaskGroupId="g1"
      selectedTaskGroupName="Default Work Lane"
    />
  )
  return onSubmit
}

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
})

describe('TaskFormModal', () => {
  test('empty title shows a visible error instead of silently ignoring the click', async () => {
    const onSubmit = renderModal()

    fireEvent.click(screen.getByRole('button', { name: /^create task$/i }))

    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent('Add a title before creating a task.')
    )
    expect(onSubmit).not.toHaveBeenCalled()
  })

  test('whitespace-only title shows the same error', async () => {
    const onSubmit = renderModal()

    fireEvent.change(screen.getByLabelText(/^title$/i), { target: { value: '   ' } })
    fireEvent.click(screen.getByRole('button', { name: /^create task$/i }))

    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent('Add a title before creating a task.')
    )
    expect(onSubmit).not.toHaveBeenCalled()
  })

  test('valid title submits and closes without an error banner', async () => {
    const onSubmit = renderModal()

    fireEvent.change(screen.getByLabelText(/^title$/i), { target: { value: 'Ship the fix' } })
    fireEvent.click(screen.getByRole('button', { name: /^create task$/i }))

    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1))
    expect(onSubmit.mock.calls[0][0]).toMatchObject({ title: 'Ship the fix', projectId: 'p1' })
    expect(screen.queryByRole('alert')).toBeNull()
  })
})
