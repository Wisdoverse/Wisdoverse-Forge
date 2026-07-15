import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { GuideDisclosure } from '@app/shared/ui/GuideDisclosure'

afterEach(cleanup)

describe('GuideDisclosure', () => {
  test('toggles an accessible body and exposes the optional dismiss action', () => {
    const onToggle = vi.fn()
    const onDismiss = vi.fn()
    const { rerender } = render(
      <GuideDisclosure
        icon={<svg data-testid="guide-icon" />}
        title="Which agent should I use?"
        expanded={true}
        onToggle={onToggle}
        onDismiss={onDismiss}
      >
        <p>Guide body</p>
      </GuideDisclosure>
    )

    const toggle = screen.getByRole('button', { name: 'Which agent should I use?' })
    expect(toggle).toHaveAttribute('aria-expanded', 'true')
    expect(screen.getByText('Guide body')).toBeInTheDocument()

    fireEvent.click(toggle)
    fireEvent.click(screen.getByRole('button', { name: 'Dismiss Which agent should I use?' }))
    expect(onToggle).toHaveBeenCalledOnce()
    expect(onDismiss).toHaveBeenCalledOnce()

    rerender(
      <GuideDisclosure
        icon={<svg data-testid="guide-icon" />}
        title="Which agent should I use?"
        expanded={false}
        onToggle={onToggle}
      >
        <p>Guide body</p>
      </GuideDisclosure>
    )

    expect(toggle).toHaveAttribute('aria-expanded', 'false')
    expect(screen.queryByText('Guide body')).toBeNull()
  })
})
