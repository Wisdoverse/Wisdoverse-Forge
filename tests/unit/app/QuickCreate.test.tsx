import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { QuickCreate } from '@app/features/board/QuickCreate'

afterEach(cleanup)

describe('QuickCreate', () => {
  test('opens with beginner guidance and a disabled create action', () => {
    render(<QuickCreate columnId="backlog" onSubmit={vi.fn()} />)

    fireEvent.click(screen.getByRole('button', { name: /add quick task/i }))

    expect(screen.getByTestId('quick-create-editor')).toBeDefined()
    expect(screen.getByLabelText('Quick task outcome')).toHaveAttribute(
      'placeholder',
      'e.g. Fix login error'
    )
    expect(
      screen.getByText(
        'Write one clear outcome. You can add details, assignee, and context after the card is created.'
      )
    ).toBeDefined()
    expect(screen.getByRole('button', { name: /create/i })).toBeDisabled()
  })

  test('submits a trimmed quick task from the explicit create button', () => {
    const onSubmit = vi.fn()
    render(<QuickCreate columnId="backlog" onSubmit={onSubmit} />)

    fireEvent.click(screen.getByRole('button', { name: /add quick task/i }))
    fireEvent.change(screen.getByLabelText('Quick task outcome'), {
      target: { value: '  Fix login error  ' },
    })
    fireEvent.click(screen.getByRole('button', { name: /create/i }))

    expect(onSubmit).toHaveBeenCalledWith('Fix login error', 'backlog')
    expect(screen.queryByTestId('quick-create-editor')).toBeNull()
    expect(screen.getByRole('button', { name: /add quick task/i })).toBeDefined()
  })

  test('cancels without creating a task', () => {
    const onSubmit = vi.fn()
    render(<QuickCreate columnId="queued" onSubmit={onSubmit} />)

    fireEvent.click(screen.getByRole('button', { name: /add quick task/i }))
    fireEvent.change(screen.getByLabelText('Quick task outcome'), {
      target: { value: 'Draft release notes' },
    })
    fireEvent.click(screen.getByRole('button', { name: /cancel/i }))

    expect(onSubmit).not.toHaveBeenCalled()
    expect(screen.queryByTestId('quick-create-editor')).toBeNull()
  })
})
