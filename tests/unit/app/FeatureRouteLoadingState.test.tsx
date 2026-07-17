import { afterEach, describe, expect, test } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { FeatureRouteLoadingState } from '@app/shared/ui/FeatureRouteLoadingState'

afterEach(cleanup)

describe('FeatureRouteLoadingState', () => {
  test('explains what is being checked and what to do next', () => {
    render(
      <FeatureRouteLoadingState
        title="Checking context items"
        detail="We are checking whether context items are available here. If this takes more than a moment, open Context again or ask an owner or admin to help with Context."
      />
    )

    expect(screen.getByRole('status')).toHaveTextContent('Checking context items')
    const panel = screen.getByText('Checking context items').closest('div')
    expect(panel?.className).toContain('rounded-md')
    expect(panel?.className).toContain('bg-transparent')
    expect(panel?.className).not.toContain('rounded-lg')
    expect(panel?.className).not.toContain('shadow-sm')
    expect(panel?.className).not.toMatch(/(^|\s)bg-white(\s|$)/)
    expect(screen.getByRole('status')).toHaveTextContent(
      'open Context again or ask an owner or admin to help with Context'
    )
    expect(screen.getByRole('status')).not.toHaveTextContent('Context access')
    expect(screen.getByRole('status')).not.toHaveTextContent('refresh the page')
    expect(screen.getByRole('status')).not.toHaveTextContent(/workspace setup/i)
    expect(screen.getByRole('status')).not.toHaveTextContent(/context\s+review/i)
  })
})
