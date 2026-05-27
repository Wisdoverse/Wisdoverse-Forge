import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { QuickCreate } from '@app/features/board/QuickCreate'

afterEach(() => {
  cleanup()
})

describe('QuickCreate', () => {
  test('opens an explicit add/cancel form', () => {
    render(<QuickCreate columnId="backlog" onSubmit={vi.fn()} />)

    fireEvent.click(screen.getByRole('button', { name: /\+ add task/i }))

    expect(screen.getByRole('textbox', { name: /task title/i })).toHaveFocus()
    expect(screen.getByRole('button', { name: /^add task$/i })).toBeDisabled()
    expect(screen.getByRole('button', { name: /^cancel$/i })).toBeEnabled()
  })

  test('does not submit when the input loses focus', () => {
    const onSubmit = vi.fn()
    render(<QuickCreate columnId="backlog" onSubmit={onSubmit} />)

    fireEvent.click(screen.getByRole('button', { name: /\+ add task/i }))
    const input = screen.getByRole('textbox', { name: /task title/i })
    fireEvent.change(input, { target: { value: 'Draft task' } })
    fireEvent.blur(input)

    expect(onSubmit).not.toHaveBeenCalled()
    expect(input).toBeInTheDocument()
  })

  test('submits with the add button and closes', () => {
    const onSubmit = vi.fn()
    render(<QuickCreate columnId="backlog" onSubmit={onSubmit} />)

    fireEvent.click(screen.getByRole('button', { name: /\+ add task/i }))
    fireEvent.change(screen.getByRole('textbox', { name: /task title/i }), {
      target: { value: '  Ship onboarding copy  ' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^add task$/i }))

    expect(onSubmit).toHaveBeenCalledWith('Ship onboarding copy', 'backlog')
    expect(screen.queryByRole('textbox', { name: /task title/i })).toBeNull()
  })

  test('keeps enter submit and escape cancel behavior', () => {
    const onSubmit = vi.fn()
    render(<QuickCreate columnId="backlog" onSubmit={onSubmit} />)

    fireEvent.click(screen.getByRole('button', { name: /\+ add task/i }))
    fireEvent.change(screen.getByRole('textbox', { name: /task title/i }), {
      target: { value: 'Keyboard task' },
    })
    fireEvent.keyDown(screen.getByRole('textbox', { name: /task title/i }), { key: 'Enter' })
    expect(onSubmit).toHaveBeenCalledWith('Keyboard task', 'backlog')

    fireEvent.click(screen.getByRole('button', { name: /\+ add task/i }))
    fireEvent.change(screen.getByRole('textbox', { name: /task title/i }), {
      target: { value: 'Canceled task' },
    })
    fireEvent.keyDown(screen.getByRole('textbox', { name: /task title/i }), { key: 'Escape' })

    expect(onSubmit).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('textbox', { name: /task title/i })).toBeNull()
  })
})
