import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { QuickCreate } from '@app/features/board/QuickCreate'

afterEach(() => {
  cleanup()
})

describe('QuickCreate', () => {
  test('opens an explicit add/cancel form', () => {
    render(<QuickCreate columnId="backlog" onSubmit={vi.fn()} />)

    fireEvent.click(screen.getByRole('button', { name: /\+ add draft task/i }))

    expect(screen.getByRole('textbox', { name: /task result/i })).toHaveFocus()
    expect(screen.getByRole('textbox', { name: /task result/i })).toHaveAccessibleDescription(
      /write one visible outcome/i
    )
    expect(screen.getByPlaceholderText(/example: fix the login error/i)).toBeDefined()
    expect(screen.getByText(/write one visible outcome/i)).toBeDefined()
    expect(screen.queryByRole('textbox', { name: /task title/i })).toBeNull()
    expect(screen.getByRole('button', { name: /^add draft task$/i })).toBeDisabled()
    expect(screen.getByRole('button', { name: /^cancel$/i })).toBeEnabled()
  })

  test('does not submit when the input loses focus', () => {
    const onSubmit = vi.fn()
    render(<QuickCreate columnId="backlog" onSubmit={onSubmit} />)

    fireEvent.click(screen.getByRole('button', { name: /\+ add draft task/i }))
    const input = screen.getByRole('textbox', { name: /task result/i })
    fireEvent.change(input, { target: { value: 'Draft task' } })
    fireEvent.blur(input)

    expect(onSubmit).not.toHaveBeenCalled()
    expect(input).toBeInTheDocument()
  })

  test('shows a next step when Enter is pressed without a task result', () => {
    const onSubmit = vi.fn()
    render(<QuickCreate columnId="backlog" onSubmit={onSubmit} />)

    fireEvent.click(screen.getByRole('button', { name: /\+ add draft task/i }))
    const input = screen.getByRole('textbox', { name: /task result/i })
    fireEvent.keyDown(input, { key: 'Enter' })

    expect(onSubmit).not.toHaveBeenCalled()
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Write the result you want before creating the draft task.'
    )
    expect(input).toHaveFocus()

    fireEvent.change(input, { target: { value: 'Fix the login error' } })
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  test('submits with the add button and closes', () => {
    const onSubmit = vi.fn()
    render(<QuickCreate columnId="backlog" onSubmit={onSubmit} />)

    fireEvent.click(screen.getByRole('button', { name: /\+ add draft task/i }))
    fireEvent.change(screen.getByRole('textbox', { name: /task result/i }), {
      target: { value: '  Ship onboarding copy  ' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^add draft task$/i }))

    expect(onSubmit).toHaveBeenCalledWith('Ship onboarding copy', 'backlog')
    expect(screen.queryByRole('textbox', { name: /task result/i })).toBeNull()
  })

  test('keeps enter submit and escape cancel behavior', () => {
    const onSubmit = vi.fn()
    render(<QuickCreate columnId="backlog" onSubmit={onSubmit} />)

    fireEvent.click(screen.getByRole('button', { name: /\+ add draft task/i }))
    fireEvent.change(screen.getByRole('textbox', { name: /task result/i }), {
      target: { value: 'Keyboard task' },
    })
    fireEvent.keyDown(screen.getByRole('textbox', { name: /task result/i }), { key: 'Enter' })
    expect(onSubmit).toHaveBeenCalledWith('Keyboard task', 'backlog')

    fireEvent.click(screen.getByRole('button', { name: /\+ add draft task/i }))
    fireEvent.change(screen.getByRole('textbox', { name: /task result/i }), {
      target: { value: 'Canceled task' },
    })
    fireEvent.keyDown(screen.getByRole('textbox', { name: /task result/i }), { key: 'Escape' })

    expect(onSubmit).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('textbox', { name: /task result/i })).toBeNull()
  })
})
