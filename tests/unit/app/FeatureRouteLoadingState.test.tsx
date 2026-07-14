import { afterEach, describe, expect, test } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { FeatureRouteLoadingState } from '@app/shared/ui/FeatureRouteLoadingState'

afterEach(cleanup)

describe('FeatureRouteLoadingState', () => {
  test('explains what is being checked and what to do next', () => {
    render(
      <FeatureRouteLoadingState
        title="Checking saved items"
        detail="We are checking whether saved items are available here. If this takes more than a moment, open Saved items again or ask an owner or admin to help with Saved items."
      />
    )

    expect(screen.getByRole('status')).toHaveTextContent('Checking saved items')
    const panel = screen.getByText('Checking saved items').closest('div')
    expect(panel?.className).toContain('rounded-md')
    expect(panel?.className).toContain('bg-transparent')
    expect(panel?.className).not.toContain('rounded-lg')
    expect(panel?.className).not.toContain('shadow-sm')
    expect(panel?.className).not.toMatch(/(^|\s)bg-white(\s|$)/)
    expect(screen.getByRole('status')).toHaveTextContent(
      'open Saved items again or ask an owner or admin to help with Saved items'
    )
    expect(screen.getByRole('status')).not.toHaveTextContent('Saved items access')
    expect(screen.getByRole('status')).not.toHaveTextContent('refresh the page')
    expect(screen.getByRole('status')).not.toHaveTextContent(/workspace setup/i)
    expect(screen.getByRole('status')).not.toHaveTextContent(/context\s+review/i)
  })
})
