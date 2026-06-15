import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { QuickCreate } from '@app/features/board/QuickCreate'

afterEach(() => {
  cleanup()
})

describe('QuickCreate', () => {
  test('opens an explicit save/cancel form', () => {
    render(<QuickCreate columnId="backlog" onSubmit={vi.fn()} />)

    fireEvent.click(screen.getByRole('button', { name: /\+ add task/i }))

    expect(screen.getByRole('textbox', { name: /task goal/i })).toHaveFocus()
    expect(screen.getByRole('textbox', { name: /task goal/i })).toHaveAccessibleDescription(
      /saves the task in not sent yet/i
    )
    expect(screen.getByPlaceholderText(/example: fix the login error/i)).toBeDefined()
    expect(screen.getByText(/open the card later to add details before sending it/i)).toBeDefined()
    expect(screen.queryByText(/quick add/i)).toBeNull()
    expect(screen.queryByRole('textbox', { name: /task title/i })).toBeNull()
    expect(screen.queryByText(/draft task/i)).toBeNull()
    expect(screen.getByRole('button', { name: /^save task$/i })).toBeDisabled()
    expect(screen.getByRole('button', { name: /^cancel$/i })).toBeEnabled()
  })

  test('does not submit when the input loses focus', () => {
    const onSubmit = vi.fn()
    render(<QuickCreate columnId="backlog" onSubmit={onSubmit} />)

    fireEvent.click(screen.getByRole('button', { name: /\+ add task/i }))
    const input = screen.getByRole('textbox', { name: /task goal/i })
    fireEvent.change(input, { target: { value: 'Task idea' } })
    fireEvent.blur(input)

    expect(onSubmit).not.toHaveBeenCalled()
    expect(input).toBeInTheDocument()
  })

  test('shows a next step when Enter is pressed without a task result', () => {
    const onSubmit = vi.fn()
    render(<QuickCreate columnId="backlog" onSubmit={onSubmit} />)

    fireEvent.click(screen.getByRole('button', { name: /\+ add task/i }))
    const input = screen.getByRole('textbox', { name: /task goal/i })
    fireEvent.keyDown(input, { key: 'Enter' })

    expect(onSubmit).not.toHaveBeenCalled()
    expect(screen.getByRole('alert')).toHaveTextContent('Write the task goal before saving it.')
    expect(input).toHaveFocus()

    fireEvent.change(input, { target: { value: 'Fix the login error' } })
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  test('submits with the add button and closes', async () => {
    const onSubmit = vi.fn()
    render(<QuickCreate columnId="backlog" onSubmit={onSubmit} />)

    fireEvent.click(screen.getByRole('button', { name: /\+ add task/i }))
    fireEvent.change(screen.getByRole('textbox', { name: /task goal/i }), {
      target: { value: '  Ship onboarding copy  ' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^save task$/i }))

    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith('Ship onboarding copy', 'backlog'))
    await waitFor(() => expect(screen.queryByRole('textbox', { name: /task goal/i })).toBeNull())
  })

  test('keeps enter submit and escape cancel behavior', async () => {
    const onSubmit = vi.fn()
    render(<QuickCreate columnId="backlog" onSubmit={onSubmit} />)

    fireEvent.click(screen.getByRole('button', { name: /\+ add task/i }))
    fireEvent.change(screen.getByRole('textbox', { name: /task goal/i }), {
      target: { value: 'Keyboard task' },
    })
    fireEvent.keyDown(screen.getByRole('textbox', { name: /task goal/i }), { key: 'Enter' })
    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith('Keyboard task', 'backlog'))
    await waitFor(() => expect(screen.queryByRole('textbox', { name: /task goal/i })).toBeNull())

    fireEvent.click(screen.getByRole('button', { name: /\+ add task/i }))
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

    fireEvent.click(screen.getByRole('button', { name: /\+ add task/i }))
    fireEvent.change(screen.getByRole('textbox', { name: /task goal/i }), {
      target: { value: 'Keep this task' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^save task$/i }))

    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith('Keep this task', 'backlog'))
    const input = screen.getByRole('textbox', { name: /task goal/i })
    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent('The task was not saved')
    )
    expect(input).toHaveValue('Keep this task')
    expect(input).toHaveFocus()
    expect(screen.getByRole('button', { name: /^save task$/i })).toBeEnabled()
  })

  test('shows a safe retry prompt when quick create throws', async () => {
    const onSubmit = vi.fn().mockRejectedValue(new Error('socket hang up'))
    render(<QuickCreate columnId="backlog" onSubmit={onSubmit} />)

    fireEvent.click(screen.getByRole('button', { name: /\+ add task/i }))
    fireEvent.change(screen.getByRole('textbox', { name: /task goal/i }), {
      target: { value: 'Retry this task' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^save task$/i }))

    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith('Retry this task', 'backlog'))
    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent(
        'The task was not saved. Check the board message, then try again.'
      )
    )
    expect(screen.getByRole('alert')).not.toHaveTextContent('socket hang up')
    expect(screen.getByRole('textbox', { name: /task goal/i })).toHaveValue('Retry this task')
  })
})
