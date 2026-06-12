import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { TaskFormModal, type TaskProjectOption } from '@app/features/board/TaskFormModal'

const PROJECTS: TaskProjectOption[] = [
  { id: 'p1', name: 'Platform', teamId: 't1', teamName: 'Core', color: '#007AFF' },
]

function renderModal(onSubmit = vi.fn()) {
  const onClose = vi.fn()
  render(
    <TaskFormModal
      isOpen
      onClose={onClose}
      onSubmit={onSubmit}
      projects={PROJECTS}
      selectedProjectId="p1"
      selectedTaskGroupId="g1"
      selectedTaskGroupName="Default Work Lane"
    />
  )
  return { onSubmit, onClose }
}

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
})

describe('TaskFormModal', () => {
  test('empty title shows a visible error instead of silently ignoring the click', async () => {
    const { onSubmit } = renderModal()

    fireEvent.click(screen.getByRole('button', { name: /^create task$/i }))

    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent('Add a title before creating a task.')
    )
    expect(onSubmit).not.toHaveBeenCalled()
  })

  test('whitespace-only title shows the same error', async () => {
    const { onSubmit } = renderModal()

    fireEvent.change(screen.getByLabelText(/^title$/i), { target: { value: '   ' } })
    fireEvent.click(screen.getByRole('button', { name: /^create task$/i }))

    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent('Add a title before creating a task.')
    )
    expect(onSubmit).not.toHaveBeenCalled()
  })

  test('valid title submits and closes without an error banner', async () => {
    const { onSubmit, onClose } = renderModal()

    fireEvent.change(screen.getByLabelText(/^title$/i), { target: { value: 'Ship the fix' } })
    fireEvent.click(screen.getByRole('button', { name: /^create task$/i }))

    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1))
    expect(onSubmit.mock.calls[0][0]).toMatchObject({ title: 'Ship the fix', projectId: 'p1' })
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1))
    expect(screen.queryByRole('alert')).toBeNull()
  })

  test('the submitted title is trimmed', async () => {
    const { onSubmit } = renderModal()

    fireEvent.change(screen.getByLabelText(/^title$/i), { target: { value: '  Ship the fix  ' } })
    fireEvent.click(screen.getByRole('button', { name: /^create task$/i }))

    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1))
    expect(onSubmit.mock.calls[0][0].title).toBe('Ship the fix')
  })

  test('an onSubmit rejection shows the error and keeps the modal open', async () => {
    const { onSubmit, onClose } = renderModal(vi.fn().mockRejectedValue(new Error('boom')))

    fireEvent.change(screen.getByLabelText(/^title$/i), { target: { value: 'Ship the fix' } })
    fireEvent.click(screen.getByRole('button', { name: /^create task$/i }))

    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('boom'))
    expect(onClose).not.toHaveBeenCalled()
    expect(onSubmit).toHaveBeenCalledTimes(1)
  })

  test('a second failed submit with the same message scrolls the banner again', async () => {
    const scrollSpy = vi
      .spyOn(Element.prototype, 'scrollIntoView')
      .mockImplementation(() => undefined)
    renderModal()
    const submit = screen.getByRole('button', { name: /^create task$/i })

    fireEvent.click(submit)
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent(/add a title/i))
    const callsAfterFirst = scrollSpy.mock.calls.length
    expect(callsAfterFirst).toBeGreaterThan(0)

    fireEvent.click(submit)
    await waitFor(() => expect(scrollSpy.mock.calls.length).toBeGreaterThan(callsAfterFirst))
    scrollSpy.mockRestore()
  })

  test('a work-lane load failure reported by onProjectChange shows a retry message', async () => {
    const onProjectChange = vi.fn().mockResolvedValue(false)
    render(
      <TaskFormModal
        isOpen
        onClose={vi.fn()}
        onSubmit={vi.fn()}
        projects={[
          ...PROJECTS,
          { id: 'p2', name: 'Other', teamId: 't1', teamName: 'Core', color: '#FF9500' },
        ]}
        selectedProjectId="p1"
        selectedTaskGroupId="g1"
        selectedTaskGroupName="Default Work Lane"
        onProjectChange={onProjectChange}
      />
    )

    fireEvent.change(screen.getByLabelText(/^project$/i), { target: { value: 'p2' } })

    await waitFor(() => expect(onProjectChange).toHaveBeenCalledWith('p2'))
    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent(/could not load work lanes/i)
    )
  })
})
