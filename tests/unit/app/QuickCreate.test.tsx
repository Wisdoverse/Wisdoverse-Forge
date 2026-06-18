import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { QuickCreate } from '@app/features/board/QuickCreate'

afterEach(() => {
  cleanup()
})

describe('QuickCreate', () => {
  test('opens an explicit save/cancel form', () => {
    render(<QuickCreate columnId="backlog" onSubmit={vi.fn()} />)

    fireEvent.click(screen.getByRole('button', { name: /add task idea/i }))

    expect(screen.getByRole('textbox', { name: /task goal/i })).toHaveFocus()
    expect(screen.getByRole('textbox', { name: /task goal/i })).toHaveAccessibleDescription(
      /only saves a draft in not sent yet/i
    )
    expect(
      screen.getByPlaceholderText(/example: fix the login error and show how to test it/i)
    ).toBeDefined()
    expect(screen.getByText(/open the card, add where to work and done when/i)).toBeDefined()
    expect(screen.getByText(/then choose an agent/i)).toBeDefined()
    expect(screen.queryByRole('button', { name: /\+ add task/i })).toBeNull()
    expect(screen.queryByText(/quick add/i)).toBeNull()
    expect(screen.queryByRole('textbox', { name: /task title/i })).toBeNull()
    expect(screen.getByRole('button', { name: /^save for later$/i })).toBeDisabled()
    expect(screen.queryByRole('button', { name: /^save task$/i })).toBeNull()
    expect(screen.getByRole('button', { name: /^cancel$/i })).toBeEnabled()
  })

  test('does not submit when the input loses focus', () => {
    const onSubmit = vi.fn()
    render(<QuickCreate columnId="backlog" onSubmit={onSubmit} />)

    fireEvent.click(screen.getByRole('button', { name: /add task idea/i }))
    const input = screen.getByRole('textbox', { name: /task goal/i })
    fireEvent.change(input, { target: { value: 'Task idea' } })
    fireEvent.blur(input)

    expect(onSubmit).not.toHaveBeenCalled()
    expect(input).toBeInTheDocument()
  })

  test('shows a next step when Enter is pressed without a task result', () => {
    const onSubmit = vi.fn()
    render(<QuickCreate columnId="backlog" onSubmit={onSubmit} />)

    fireEvent.click(screen.getByRole('button', { name: /add task idea/i }))
    const input = screen.getByRole('textbox', { name: /task goal/i })
    fireEvent.keyDown(input, { key: 'Enter' })

    expect(onSubmit).not.toHaveBeenCalled()
    expect(screen.getByRole('alert')).toHaveTextContent('Write the task goal before saving it.')
    expect(input).toHaveFocus()

    fireEvent.change(input, {
      target: { value: 'Fix the login error and show how to test it' },
    })
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  test('submits with the add button and closes', async () => {
    const onSubmit = vi.fn()
    render(<QuickCreate columnId="backlog" onSubmit={onSubmit} />)

    fireEvent.click(screen.getByRole('button', { name: /add task idea/i }))
    fireEvent.change(screen.getByRole('textbox', { name: /task goal/i }), {
      target: { value: '  Ship onboarding copy  ' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^save for later$/i }))

    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith('Ship onboarding copy', 'backlog'))
    await waitFor(() => expect(screen.queryByRole('textbox', { name: /task goal/i })).toBeNull())
  })

  test('keeps enter submit and escape cancel behavior', async () => {
    const onSubmit = vi.fn()
    render(<QuickCreate columnId="backlog" onSubmit={onSubmit} />)

    fireEvent.click(screen.getByRole('button', { name: /add task idea/i }))
    fireEvent.change(screen.getByRole('textbox', { name: /task goal/i }), {
      target: { value: 'Keyboard task' },
    })
    fireEvent.keyDown(screen.getByRole('textbox', { name: /task goal/i }), { key: 'Enter' })
    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith('Keyboard task', 'backlog'))
    await waitFor(() => expect(screen.queryByRole('textbox', { name: /task goal/i })).toBeNull())

    fireEvent.click(screen.getByRole('button', { name: /add task idea/i }))
    fireEvent.change(screen.getByRole('textbox', { name: /task goal/i }), {
      target: { value: 'Canceled task' },
    })
    fireEvent.keyDown(screen.getByRole('textbox', { name: /task goal/i }), { key: 'Escape' })

    expect(onSubmit).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('textbox', { name: /task goal/i })).toBeNull()
  })

  test('keeps the task form open when quick create fails', async () => {
    const onSubmit = vi.fn().mockResolvedValue(false)
    render(<QuickCreate columnId="backlog" onSubmit={onSubmit} />)

    fireEvent.click(screen.getByRole('button', { name: /add task idea/i }))
    fireEvent.change(screen.getByRole('textbox', { name: /task goal/i }), {
      target: { value: 'Keep this task' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^save for later$/i }))

    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith('Keep this task', 'backlog'))
    const input = screen.getByRole('textbox', { name: /task goal/i })
    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent('The task was not saved')
    )
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Check the project, where tasks wait, and your connection'
    )
    expect(input).toHaveValue('Keep this task')
    expect(input).toHaveFocus()
    expect(screen.getByRole('button', { name: /^save for later$/i })).toBeEnabled()
  })

  test('shows the specific recovery prompt returned by the board', async () => {
    const onSubmit = vi
      .fn()
      .mockResolvedValue(
        'Check the project, task queue, and result, then create the task again. The task was not created.'
      )
    render(<QuickCreate columnId="backlog" onSubmit={onSubmit} />)

    fireEvent.click(screen.getByRole('button', { name: /add task idea/i }))
    fireEvent.change(screen.getByRole('textbox', { name: /task goal/i }), {
      target: { value: 'Recover this task' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^save for later$/i }))

    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith('Recover this task', 'backlog'))
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Check the project, task queue, and result, then create the task again.'
    )
    expect(screen.getByRole('textbox', { name: /task goal/i })).toHaveValue('Recover this task')
    expect(screen.getByRole('textbox', { name: /task goal/i })).toHaveFocus()
  })

  test('shows a safe retry prompt when quick create throws', async () => {
    const onSubmit = vi.fn().mockRejectedValue(new Error('socket hang up'))
    render(<QuickCreate columnId="backlog" onSubmit={onSubmit} />)

    fireEvent.click(screen.getByRole('button', { name: /add task idea/i }))
    fireEvent.change(screen.getByRole('textbox', { name: /task goal/i }), {
      target: { value: 'Retry this task' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^save for later$/i }))

    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith('Retry this task', 'backlog'))
    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent(
        'The task was not saved. Check the project, where tasks wait, and your connection, then choose Save for later again.'
      )
    )
    expect(screen.getByRole('alert')).not.toHaveTextContent('socket hang up')
    expect(screen.getByRole('textbox', { name: /task goal/i })).toHaveValue('Retry this task')
  })
})
