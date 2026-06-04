import { afterEach, describe, expect, test } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { FeatureRouteLoadingState } from '@app/shared/ui/FeatureRouteLoadingState'

afterEach(cleanup)

describe('FeatureRouteLoadingState', () => {
  test('explains what is being checked and what to do next', () => {
    render(
      <FeatureRouteLoadingState
        title="Checking context review"
        detail="We are confirming whether context review is enabled for this workspace. If this takes more than a moment, refresh the page or ask an owner or admin to check workspace setup."
      />
    )

    expect(screen.getByRole('status')).toHaveTextContent('Checking context review')
    expect(screen.getByRole('status')).toHaveTextContent(
      'refresh the page or ask an owner or admin to check workspace setup'
    )
  })
})
