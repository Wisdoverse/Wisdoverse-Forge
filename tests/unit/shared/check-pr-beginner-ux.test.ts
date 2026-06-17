import { describe, expect, it } from 'vitest'

import { checkPullRequestBody } from '../../../scripts/check-pr-beginner-ux.mjs'

function pr(body: string, overrides: Record<string, unknown> = {}) {
  return {
    body,
    head: { ref: 'codex/example' },
    user: { login: 'contributor' },
    ...overrides,
  }
}

const completeBody = `
## Summary
- Improve a visible workflow.

## Beginner UX / First-Time User Path

- Shortest safe path: Open Settings, choose Runtime, and connect the missing credential first.
- Prerequisites shown before action: The screen shows project, runtime, and provider readiness before submit.
- Success looks like: The user sees Ready to assign and can open the task board.
- Error or recovery path: Errors include a retry button and a link back to setup.
- Destructive or permission impact: No destructive action or permission change is included.
- CLI platforms covered, if applicable: Not applicable because this is browser-only.
`

describe('check-pr-beginner-ux.mjs', () => {
  it('accepts a complete beginner UX section', () => {
    const result = checkPullRequestBody(pr(completeBody))

    expect(result.ok).toBe(true)
    expect(result.message).toBe('Beginner UX / First-Time User Path section is complete.')
    expect(result.errors).toEqual([])
  })

  it('rejects the old user-path heading so new PRs use beginner-facing language', () => {
    const oldHeading = ['## Beginner UX /', 'Operator Path'].join(' ')
    const result = checkPullRequestBody(
      pr(completeBody.replace('## Beginner UX / First-Time User Path', oldHeading))
    )

    expect(result.ok).toBe(false)
    expect(result.errors).toContain('Missing "## Beginner UX / First-Time User Path" section.')
  })

  it('fails when the section is missing', () => {
    const result = checkPullRequestBody(pr('## Summary\n- Internal change.'))

    expect(result.ok).toBe(false)
    expect(result.errors).toContain('Missing "## Beginner UX / First-Time User Path" section.')
  })

  it('fails when fields are left as placeholders', () => {
    const result = checkPullRequestBody(
      pr(`
## Beginner UX / First-Time User Path

- Shortest safe path:
- Prerequisites shown before action: TBD
- Success looks like: Done
- Error or recovery path: n/a
- Destructive or permission impact: none
- CLI platforms covered, if applicable: N/A
`)
    )

    expect(result.ok).toBe(false)
    expect(result.errors).toContain('Field needs a concrete value: Shortest safe path')
    expect(result.errors).toContain(
      'Field needs a concrete value: Prerequisites shown before action'
    )
  })

  it('accepts non-user-facing changes with an explanation', () => {
    const result = checkPullRequestBody(
      pr(`
## Beginner UX / First-Time User Path

Not user-facing: internal test fixture update only, no user-visible behavior changes.
`)
    )

    expect(result.ok).toBe(true)
  })

  it('skips Dependabot pull requests', () => {
    const result = checkPullRequestBody(
      pr('', {
        head: { ref: 'dependabot/npm_and_yarn/example' },
        user: { login: 'dependabot[bot]' },
      })
    )

    expect(result.ok).toBe(true)
    expect(result.skipped).toBe(true)
  })
})
